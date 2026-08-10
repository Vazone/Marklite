<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { get } from 'svelte/store';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import MarkdownEditor from '../components/editor/MarkdownEditor.svelte';
  import PreviewPane from '../components/editor/PreviewPane.svelte';
  import Sidebar from '../components/layout/Sidebar.svelte';
  import DocumentTabBar from '../components/layout/DocumentTabBar.svelte';
  import SplitPaneSeparator from '../components/layout/SplitPaneSeparator.svelte';
  import StatusBar from '../components/layout/StatusBar.svelte';
  import TitleBar from '../components/layout/TitleBar.svelte';
  import CommandPalette from '../components/layout/CommandPalette.svelte';
  import SettingsDialog from '../components/dialogs/SettingsDialog.svelte';
  import AboutDialog from '../components/dialogs/AboutDialog.svelte';
  import {
    activeTab,
    documentStore,
    getActiveTab,
    type CursorPosition,
    type DocumentState,
    type EditorScrollPosition,
    type EditorTab
  } from './stores/documentStore';
  import { settingsStore } from './stores/settingsStore';
  import { uiActions, uiStore, type LayoutMode } from './stores/uiStore';
  import {
    api,
    confirmAction,
    isTauriRuntime,
    pickHtmlSavePath,
    pickMarkdownFile,
    pickMarkdownSavePath,
    pickStartupDiagnosticsSavePath,
    toAppError,
    type AppSettings,
    type RecentFileDto,
    type SessionStateDto
  } from '../lib/tauriApi';
  import { toolbarItems, type ToolbarAction } from '../lib/markdownToolbar';
  import type { CommandItem } from '../lib/commands';
  import { createAsyncSingleFlight } from '../lib/asyncSingleFlight';
  import { applyDocumentOperation } from '../lib/documentOperation';
  import { startupElapsedMs, type FrontendStartupStage } from '../lib/startupLifecycle';

  let editorRef: any;
  let previewRef: any;
  let recentFiles: RecentFileDto[] = [];
  let renderTimer: number | undefined;
  let autosaveTimer: number | undefined;
  let sessionTimer: number | undefined;
  let autosaveKey = '';
  let lastRenderKey = '';
  let lastSessionKey = '';
  let sessionReady = false;
  let pendingPreviewFragment: { tabId: string; fragment: string } | null = null;
  const saveSingleFlight = createAsyncSingleFlight<string>();
  const loadSingleFlight = createAsyncSingleFlight<string>();
  const autosaveFailureKeys = new Map<string, string>();
  let unlistenDrop: (() => void) | undefined;
  let unlistenSingleInstance: (() => void) | undefined;
  const initializationStartedAt = performance.now();

  $: currentTitle = $activeTab?.title ?? 'Untitled.md';
  $: effectiveSidebarVisible = $uiStore.sidebarVisible && $settingsStore.showSidebar;
  $: commandItems = buildCommands();

  $: if ($settingsStore) {
    applyTheme($settingsStore);
  }

  $: if ($activeTab?.loadState === 'loaded' && $settingsStore.livePreviewEnabled) {
    const renderKey = `${$activeTab.id}:${$activeTab.contentRevision}`;
    if (renderKey !== lastRenderKey) {
      lastRenderKey = renderKey;
      scheduleRender($activeTab.id, $activeTab.content, $activeTab.contentRevision);
    }
  }

  $: {
    const nextKey = `${$settingsStore.autosaveEnabled}:${$settingsStore.autosaveIntervalMs}:${$activeTab?.id ?? ''}`;
    if (nextKey !== autosaveKey) {
      autosaveKey = nextKey;
      if (autosaveTimer) window.clearInterval(autosaveTimer);
      autosaveTimer = undefined;

      if ($settingsStore.autosaveEnabled) {
        autosaveTimer = window.setInterval(() => {
          const tab = getActiveTab();
          if (tab?.path && tab.loadState === 'loaded' && tab.isDirty) {
            void saveActive(false);
          }
        }, Math.max(1000, $settingsStore.autosaveIntervalMs));
      }
    }
  }

  $: if (sessionReady) {
    const session = currentSessionSnapshot($documentStore);
    const sessionKey = `${$settingsStore.restoreLastSession}:${session.activePath ?? ''}:${session.paths.join('\u0000')}`;
    if (sessionKey !== lastSessionKey) {
      lastSessionKey = sessionKey;
      scheduleSessionPersist($settingsStore.restoreLastSession, session);
    }
  }

  onMount(() => {
    void initialize().catch((error) => {
      uiActions.toast(`启动初始化未完全完成：${toAppError(error).message}`, 'error');
    });

    const keyHandler = (event: KeyboardEvent) => {
      handleGlobalShortcut(event);
    };
    const beforeUnload = (event: BeforeUnloadEvent) => {
      if (get(documentStore).tabs.some((tab) => tab.isDirty)) {
        event.preventDefault();
        event.returnValue = '';
      }
    };

    window.addEventListener('keydown', keyHandler);
    window.addEventListener('beforeunload', beforeUnload);

    return () => {
      window.removeEventListener('keydown', keyHandler);
      window.removeEventListener('beforeunload', beforeUnload);
      unlistenDrop?.();
      unlistenSingleInstance?.();
    };
  });

  onDestroy(() => {
    if (renderTimer) window.clearTimeout(renderTimer);
    if (autosaveTimer) window.clearInterval(autosaveTimer);
    if (sessionTimer) window.clearTimeout(sessionTimer);
  });

  async function initialize() {
    recordStartupStage('initialization', 'started');
    try {
      await runStartupStage('settings', async () => {
        await settingsStore.load();
        const settings = get(settingsStore);
        uiActions.setSidebarVisible(settings.showSidebar);
      });
      const settings = get(settingsStore);
      await runStartupStage('recentFiles', refreshRecentFiles);
      await runStartupStage('sessionRestore', () => restoreSession(settings.restoreLastSession));
      sessionReady = true;
      await runStartupStage('externalListeners', setupExternalOpenListener);
      await runStartupStage('startupFile', openStartupFile);
      await runStartupStage('firstRender', renderActiveNow);
      await runStartupStage('dragDrop', setupDragDrop);
      recordStartupStage('initialization', 'succeeded');
    } catch (error) {
      recordStartupStage('initialization', 'failed', 'initializationFailed');
      throw error;
    }
  }

  async function runStartupStage(stage: FrontendStartupStage, operation: () => Promise<void>) {
    recordStartupStage(stage, 'started');
    try {
      await operation();
      recordStartupStage(stage, 'succeeded');
    } catch (error) {
      recordStartupStage(stage, 'failed', 'initializationFailed');
      throw error;
    }
  }

  function recordStartupStage(
    stage: FrontendStartupStage,
    status: 'started' | 'succeeded' | 'failed',
    code: 'initializationFailed' | null = null
  ) {
    void api
      .recordFrontendStartupEvent({
        stage,
        status,
        code,
        elapsedMs: startupElapsedMs(initializationStartedAt)
      })
      .catch(() => undefined);
  }

  async function restoreSession(enabled: boolean) {
    if (!enabled) {
      try {
        await api.clearSession();
      } catch (error) {
        uiActions.toast(toAppError(error).message, 'error');
      }
      return;
    }

    try {
      const session = await api.getSession();
      documentStore.restoreSessionTabs(session.paths.slice(0, 50), session.activePath);
    } catch (error) {
      uiActions.toast(toAppError(error).message, 'error');
    }
  }

  function currentSessionSnapshot(state: DocumentState): SessionStateDto {
    const paths = [...new Set(state.tabs.map((tab) => tab.path).filter((path): path is string => Boolean(path)))].slice(0, 50);
    const activePath = state.tabs.find((tab) => tab.id === state.activeTabId)?.path ?? null;
    return {
      version: 1,
      paths,
      activePath: activePath && paths.includes(activePath) ? activePath : null
    };
  }

  function scheduleSessionPersist(enabled: boolean, session: SessionStateDto) {
    if (sessionTimer) window.clearTimeout(sessionTimer);
    sessionTimer = window.setTimeout(() => {
      sessionTimer = undefined;
      void persistSession(enabled, session);
    }, 200);
  }

  async function persistSession(enabled: boolean, session: SessionStateDto) {
    try {
      if (enabled) {
        await api.updateSession(session);
      } else {
        await api.clearSession();
      }
    } catch (error) {
      uiActions.toast(toAppError(error).message, 'error');
    }
  }

  async function openStartupFile() {
    try {
      const path = await api.getStartupFileArg();
      if (path) {
        await openPath(path);
      }
    } catch (error) {
      console.warn('No startup file argument available', error);
    }
  }

  async function setupDragDrop() {
    if (!isTauriRuntime()) return;

    try {
      unlistenDrop = await getCurrentWindow().onDragDropEvent((event: any) => {
        if (event.payload?.type !== 'drop') return;
        const path = event.payload.paths?.find((item: string) => /\.(md|markdown|txt)$/i.test(item));
        if (path) {
          void openPath(path);
        }
      });
    } catch (error) {
      console.warn('Failed to register drag drop handler', error);
    }
  }

  async function setupExternalOpenListener() {
    if (!isTauriRuntime()) return;

    try {
      unlistenSingleInstance = await getCurrentWindow().listen<string>('single-instance-open-file', (event) => {
        if (event.payload) {
          void openPath(event.payload);
        }
      });
    } catch (error) {
      console.warn('Failed to register external open listener', error);
    }
  }

  function applyTheme(settings: AppSettings) {
    if (typeof document === 'undefined') return;

    const prefersDark = window.matchMedia?.('(prefers-color-scheme: dark)').matches ?? false;
    const theme = settings.theme === 'system' ? (prefersDark ? 'dark' : 'light') : settings.theme;
    const root = document.documentElement;
    root.dataset.theme = theme;
    root.style.setProperty('--accent-color', settings.accentColor);
    root.style.setProperty('--radius-md', `${settings.cornerRadius}px`);
    root.style.setProperty('--interface-scale', String(settings.interfaceScale));
  }

  function scheduleRender(tabId: string, content: string, contentRevision: number) {
    if (renderTimer) window.clearTimeout(renderTimer);
    renderTimer = window.setTimeout(() => {
      void renderTab(tabId, content, contentRevision);
    }, Math.max(100, $settingsStore.previewDebounceMs));
  }

  async function renderActiveNow() {
    const tab = getActiveTab();
    if (tab?.loadState === 'loaded') {
      await renderTab(tab.id, tab.content, tab.contentRevision);
    }
  }

  async function renderTab(tabId: string, content: string, contentRevision: number) {
    try {
      const rendered = await api.renderMarkdown(content);
      const applied = documentStore.updateRendered(tabId, contentRevision, rendered);
      if (applied && pendingPreviewFragment?.tabId === tabId) {
        const fragment = pendingPreviewFragment.fragment;
        pendingPreviewFragment = null;
        window.setTimeout(() => previewRef?.scrollToFragment(fragment), 0);
      }
      if (applied && tabId === getActiveTab()?.id && $settingsStore.syncScroll) {
        const position = getActiveTab()?.scrollPosition;
        window.setTimeout(() => previewRef?.syncToEditorScroll(position), 0);
      }
    } catch (error) {
      if (documentStore.isCurrentRevision(tabId, contentRevision)) {
        uiActions.toast(toAppError(error).message, 'error');
      }
    }
  }

  async function refreshRecentFiles() {
    try {
      recentFiles = await api.getRecentFiles();
    } catch (error) {
      uiActions.toast(toAppError(error).message, 'error');
    }
  }

  function newDocument() {
    documentStore.newDocument();
    uiActions.toast('已新建文档');
  }

  async function openFile() {
    try {
      const path = await pickMarkdownFile();
      if (path) {
        await openPath(path);
      }
    } catch (error) {
      uiActions.toast(toAppError(error).message, 'error');
    }
  }

  async function openPath(path: string): Promise<EditorTab | null> {
    try {
      const result = await api.openMarkdownFile(path);
      const auxiliaryError = applyDocumentOperation(result, documentStore.openDocument);
      await refreshRecentFiles();
      if (auxiliaryError) {
        uiActions.toast(`已打开 ${result.document.title}，但最近文件未更新：${auxiliaryError.message}`, 'error');
      } else {
        uiActions.toast(`已打开 ${result.document.title}`);
      }
      return getActiveTab() ?? null;
    } catch (error) {
      uiActions.toast(toAppError(error).message, 'error');
      return null;
    }
  }

  async function activateTab(tabId: string) {
    documentStore.setActive(tabId);
    await loadTabContent(tabId);
  }

  async function loadTabContent(tabId: string): Promise<EditorTab | null> {
    const request = loadSingleFlight.run(tabId, async () => {
      const tab = documentStore.getTab(tabId);
      if (!tab) return null;
      if (tab.loadState === 'loaded') return tab;
      if (!tab.path) {
        documentStore.markLoadFailed(tabId, '该标签没有可读取的文件路径');
        return null;
      }

      const requestedPath = tab.path;
      documentStore.markLoading(tabId);
      try {
        const result = await api.openMarkdownFile(requestedPath);
        const applied = documentStore.hydrateDocument(tabId, requestedPath, result.document);
        if (!applied) return documentStore.getTab(tabId) ?? null;
        await refreshRecentFiles();
        if (result.auxiliaryError) {
          uiActions.toast(`已加载 ${result.document.title}，但最近文件未更新：${result.auxiliaryError.message}`, 'error');
        }
        return documentStore.getTab(tabId) ?? null;
      } catch (error) {
        const appError = toAppError(error);
        documentStore.markLoadFailed(tabId, appError.message);
        uiActions.toast(appError.message, 'error');
        return null;
      }
    });
    return request.promise;
  }

  async function openPreviewDocument(path: string, fragment: string | null) {
    const tab = await openPath(path);
    if (!tab || !fragment) return;
    pendingPreviewFragment = { tabId: tab.id, fragment };
    if (tab.renderedRevision === tab.contentRevision) {
      pendingPreviewFragment = null;
      window.setTimeout(() => previewRef?.scrollToFragment(fragment), 0);
    }
  }

  async function saveActive(showToast = true) {
    let tab = getActiveTab();
    if (!tab) return;
    if (tab.loadState !== 'loaded') {
      const loadedTab = await loadTabContent(tab.id);
      if (!loadedTab || loadedTab.loadState !== 'loaded') return;
      tab = loadedTab;
    }

    try {
      const path = tab.path ?? (await pickMarkdownSavePath(tab.title));
      if (!path) return;
      await saveTab(tab.id, path, showToast);
    } catch (error) {
      uiActions.toast(toAppError(error).message, 'error');
    }
  }

  async function saveActiveAs() {
    let tab = getActiveTab();
    if (!tab) return;
    if (tab.loadState !== 'loaded') {
      const loadedTab = await loadTabContent(tab.id);
      if (!loadedTab || loadedTab.loadState !== 'loaded') return;
      tab = loadedTab;
    }

    try {
      const path = await pickMarkdownSavePath(tab.path ?? tab.title);
      if (!path) return;
      await saveTab(tab.id, path, true);
    } catch (error) {
      uiActions.toast(toAppError(error).message, 'error');
    }
  }

  async function saveTab(tabId: string, path: string, showToast: boolean): Promise<void> {
    const request = saveSingleFlight.run(tabId, async () => {
      const tab = documentStore.getTab(tabId);
      if (!tab || tab.loadState !== 'loaded') return null;

      const contentRevision = tab.contentRevision;
      const result = await api.saveMarkdownFile(path, tab.content);
      applyDocumentOperation(result, (document) => {
        documentStore.markSaved(tabId, contentRevision, document);
      });
      await refreshRecentFiles();
      return result;
    });

    try {
      const result = await request.promise;
      if (!result) return;
      const saved = result.document;

      const current = documentStore.getTab(tabId);
      if (!request.started && showToast && current && (current.isDirty || current.path !== path)) {
        await saveTab(tabId, path, true);
        return;
      }

      const savedMessage = current?.isDirty
        ? `已保存 ${saved.title} 的快照，仍有未保存更改`
        : `已保存 ${saved.title}`;
      if (result.auxiliaryError) {
        const failureKey = `${result.auxiliaryError.code}:${result.auxiliaryError.message}`;
        if (showToast || autosaveFailureKeys.get(tabId) !== failureKey) {
          uiActions.toast(`${savedMessage}；最近文件未更新：${result.auxiliaryError.message}`, 'error');
        }
        autosaveFailureKeys.set(tabId, failureKey);
      } else {
        autosaveFailureKeys.delete(tabId);
        if (showToast) {
          uiActions.toast(savedMessage, current?.isDirty ? 'info' : 'success');
        }
      }
    } catch (error) {
      const appError = toAppError(error);
      const failureKey = `${appError.code}:${appError.message}`;
      if (showToast || autosaveFailureKeys.get(tabId) !== failureKey) {
        uiActions.toast(appError.message, 'error');
      }
      autosaveFailureKeys.set(tabId, failureKey);
    }
  }

  async function exportHtml() {
    let tab = getActiveTab();
    if (!tab) return;
    if (tab.loadState !== 'loaded') {
      const loadedTab = await loadTabContent(tab.id);
      if (!loadedTab || loadedTab.loadState !== 'loaded') return;
      tab = loadedTab;
    }

    try {
      const fallbackName = tab.path
        ? tab.path.replace(/\.(md|markdown|txt)$/i, '.html')
        : `${tab.title.replace(/\.(md|markdown|txt)$/i, '')}.html`;
      const path = await pickHtmlSavePath(fallbackName);
      if (!path) return;
      await api.exportHtmlFile(path, tab.title, tab.content);
      uiActions.toast('HTML 已导出');
    } catch (error) {
      uiActions.toast(toAppError(error).message, 'error');
    }
  }

  async function exportStartupDiagnostics() {
    try {
      const path = await pickStartupDiagnosticsSavePath();
      if (!path) return;
      const result = await api.exportStartupDiagnostics(path);
      uiActions.toast(`已导出 ${result.recordCount} 条启动诊断`);
    } catch (error) {
      uiActions.toast(toAppError(error).message, 'error');
    }
  }

  async function clearStartupDiagnostics() {
    try {
      await api.clearStartupDiagnostics();
      uiActions.toast('启动诊断已清除');
    } catch (error) {
      uiActions.toast(toAppError(error).message, 'error');
    }
  }

  async function closeTab(tab: EditorTab) {
    if (tab.isDirty) {
      const confirmed = await confirmAction(`“${tab.title}” 尚未保存，确定关闭？`, '关闭未保存文件');
      if (!confirmed) return;
    }
    documentStore.closeTab(tab.id);
  }

  async function closeTabById(tabId: string) {
    const tab = documentStore.getTab(tabId);
    if (tab) await closeTab(tab);
  }

  async function closeOtherTabs(sourceTabId: string) {
    const state = get(documentStore);
    const targets = state.tabs.filter((tab) => tab.id !== sourceTabId);
    await closeTabGroup(targets, sourceTabId, '关闭其他标签页');
  }

  async function closeRightTabs(sourceTabId: string) {
    const state = get(documentStore);
    const sourceIndex = state.tabs.findIndex((tab) => tab.id === sourceTabId);
    if (sourceIndex < 0) return;
    await closeTabGroup(state.tabs.slice(sourceIndex + 1), sourceTabId, '关闭右侧标签页');
  }

  async function closeTabGroup(tabs: EditorTab[], preferredActiveId: string, actionTitle: string) {
    if (!tabs.length) return;
    const dirtyTabs = tabs.filter((tab) => tab.isDirty);
    if (dirtyTabs.length) {
      const confirmed = await confirmAction(
        `${dirtyTabs.length} 个标签包含未保存更改，确定${actionTitle}？`,
        actionTitle
      );
      if (!confirmed) return;
    }
    documentStore.closeTabs(
      tabs.map((tab) => tab.id),
      preferredActiveId
    );
  }

  function setLayoutMode(mode: LayoutMode) {
    uiActions.setLayoutMode(mode);
  }

  function handleContentChange(tabId: string, content: string) {
    documentStore.updateContent(tabId, content);
  }

  function handleCursorChange(tabId: string, position: CursorPosition) {
    documentStore.updateCursor(tabId, position);
  }

  function handleEditorScroll(tabId: string, position: EditorScrollPosition) {
    documentStore.updateScroll(tabId, position);
    if (tabId === getActiveTab()?.id && $settingsStore.syncScroll && $uiStore.layoutMode !== 'edit') {
      previewRef?.syncToEditorScroll(position);
    }
  }

  function handleEditorSession(tabId: string, state: import('../lib/editorSession').SerializedEditorState) {
    documentStore.updateEditorState(tabId, state);
  }

  function applyToolbar(action: ToolbarAction) {
    editorRef?.applyMarkdown(action);
  }

  function jumpToLine(line: number) {
    if ($uiStore.layoutMode === 'preview') {
      uiActions.setLayoutMode('split');
    }
    window.setTimeout(() => editorRef?.jumpToLine(line), 0);
  }

  async function removeRecent(path: string) {
    try {
      recentFiles = await api.removeRecentFile(path);
    } catch (error) {
      uiActions.toast(toAppError(error).message, 'error');
    }
  }

  async function revealRecent(path: string) {
    try {
      await api.showInFileManager(path);
    } catch (error) {
      uiActions.toast(toAppError(error).message, 'error');
    }
  }

  async function saveSettings(settings: AppSettings) {
    let saved: AppSettings;
    try {
      saved = await settingsStore.save(settings);
    } catch {
      return;
    }
    if (!saved.restoreLastSession) {
      try {
        await api.clearSession();
      } catch (error) {
        uiActions.toast(toAppError(error).message, 'error');
        return;
      }
    }
    uiActions.setSidebarVisible(saved.showSidebar);
    uiActions.closeSettings();
  }

  function resetSettings() {
    void settingsStore.reset().catch(() => undefined);
  }

  function handleGlobalShortcut(event: KeyboardEvent) {
    const isModifier = event.ctrlKey || event.metaKey;
    if (!isModifier) return;

    const key = event.key.toLowerCase();

    if (key === 'n') {
      event.preventDefault();
      newDocument();
    } else if (key === 'o') {
      event.preventDefault();
      void openFile();
    } else if (key === 's' && event.shiftKey) {
      event.preventDefault();
      void saveActiveAs();
    } else if (key === 's') {
      event.preventDefault();
      void saveActive();
    } else if (key === 'e') {
      event.preventDefault();
      uiActions.togglePreview();
    } else if (key === ',') {
      event.preventDefault();
      uiActions.openSettings();
    } else if (key === 'p') {
      event.preventDefault();
      uiActions.openCommandPalette();
    } else if (key === 'w') {
      event.preventDefault();
      const tab = getActiveTab();
      if (tab) void closeTab(tab);
    } else if (key === 'f') {
      event.preventDefault();
      editorRef?.openFind();
    } else if (key === 'h') {
      event.preventDefault();
      editorRef?.openReplace();
    }
  }

  function buildCommands(): CommandItem[] {
    return [
      { id: 'new', title: '新建文件', category: '文件', shortcut: 'Ctrl+N', action: newDocument },
      { id: 'open', title: '打开文件', category: '文件', shortcut: 'Ctrl+O', action: () => void openFile() },
      { id: 'save', title: '保存文件', category: '文件', shortcut: 'Ctrl+S', action: () => void saveActive() },
      {
        id: 'save-as',
        title: '另存为',
        category: '文件',
        shortcut: 'Ctrl+Shift+S',
        action: () => void saveActiveAs()
      },
      { id: 'export-html', title: '导出 HTML', category: '工具', action: () => void exportHtml() },
      { id: 'layout-edit', title: '编辑模式', category: '视图', action: () => setLayoutMode('edit') },
      { id: 'layout-split', title: '分栏模式', category: '视图', action: () => setLayoutMode('split') },
      { id: 'layout-preview', title: '预览模式', category: '视图', shortcut: 'Ctrl+E', action: () => setLayoutMode('preview') },
      { id: 'sidebar', title: '显示或隐藏侧边栏', category: '视图', action: uiActions.toggleSidebar },
      { id: 'settings', title: '打开设置', category: '工具', shortcut: 'Ctrl+,', action: uiActions.openSettings },
      { id: 'export-startup-diagnostics', title: '导出启动诊断', category: '帮助', action: () => void exportStartupDiagnostics() },
      { id: 'clear-startup-diagnostics', title: '清除启动诊断', category: '帮助', action: () => void clearStartupDiagnostics() },
      { id: 'about', title: '关于 MarkLite', category: '帮助', action: uiActions.openAbout }
    ];
  }
</script>

<svelte:head>
  <title>{currentTitle} - MarkLite</title>
</svelte:head>

<div class="app-shell" data-marklite-ready="true" aria-label="MarkLite 编辑器">
  <TitleBar
    title={currentTitle}
    isDirty={$activeTab?.isDirty}
    layoutMode={$uiStore.layoutMode}
    sidebarVisible={effectiveSidebarVisible}
    onNew={newDocument}
    onOpen={() => void openFile()}
    onSave={() => void saveActive()}
    onSaveAs={() => void saveActiveAs()}
    onExportHtml={() => void exportHtml()}
    onFind={() => editorRef?.openFind()}
    onSettings={uiActions.openSettings}
    onToggleSidebar={uiActions.toggleSidebar}
    onLayout={setLayoutMode}
    onCommandPalette={uiActions.openCommandPalette}
    onAbout={uiActions.openAbout}
  />

  <DocumentTabBar
    tabs={$documentStore.tabs}
    activeTabId={$documentStore.activeTabId}
    onActivate={(tabId) => void activateTab(tabId)}
    onClose={(tabId) => void closeTabById(tabId)}
    onCloseOthers={(tabId) => void closeOtherTabs(tabId)}
    onCloseRight={(tabId) => void closeRightTabs(tabId)}
  />

  {#if $settingsStore.markdownToolbarEnabled && $uiStore.layoutMode !== 'preview' && $activeTab?.loadState === 'loaded'}
    <div class="markdown-toolbar" aria-label="Markdown 工具栏">
      {#each toolbarItems as item}
        <button type="button" title={item.shortcut ? `${item.label} ${item.shortcut}` : item.label} on:click={() => applyToolbar(item.action)}>
          <svelte:component this={item.icon} size={16} />
        </button>
      {/each}
    </div>
  {/if}

  <div class="workspace" class:no-sidebar={!effectiveSidebarVisible}>
    {#if effectiveSidebarVisible}
      <Sidebar
        activeSidebarTab={$uiStore.sidebarTab}
        {recentFiles}
        tab={$activeTab}
        onTabChange={uiActions.setSidebarTab}
        onOpenRecent={(path) => void openPath(path)}
        onRemoveRecent={(path) => void removeRecent(path)}
        onRevealRecent={(path) => void revealRecent(path)}
        onJumpToLine={jumpToLine}
      />
    {/if}

    <main
      class={`editor-stage layout-${$uiStore.layoutMode}`}
      style={`--split-left: ${($uiStore.splitRatio * 100).toFixed(4)}%; --split-right: ${((1 - $uiStore.splitRatio) * 100).toFixed(4)}%;`}
    >
      {#if $activeTab && $activeTab.loadState !== 'loaded'}
        <section class="document-load-state" aria-live="polite">
          {#if $activeTab.loadState === 'loading'}
            <strong>正在加载 {$activeTab.title}…</strong>
            <span>只读取当前选中的文档，其余恢复标签仍保持休眠。</span>
          {:else if $activeTab.loadState === 'error'}
            <strong>无法加载 {$activeTab.title}</strong>
            <span>{$activeTab.loadError ?? '文件暂时不可用'}</span>
            <button type="button" class="primary-button" on:click={() => void loadTabContent($activeTab.id)}>重试加载</button>
          {:else}
            <strong>{$activeTab.title}</strong>
            <span>此恢复标签尚未读取正文。</span>
            <button type="button" class="primary-button" on:click={() => void loadTabContent($activeTab.id)}>加载文档</button>
          {/if}
        </section>
      {:else}
        {#if $activeTab && ($uiStore.layoutMode === 'edit' || $uiStore.layoutMode === 'split')}
          <section class="editor-pane">
            {#key $activeTab.id}
              <MarkdownEditor
                bind:this={editorRef}
                tabId={$activeTab.id}
                value={$activeTab.content}
                settings={$settingsStore}
                serializedState={$activeTab.editorState}
                initialScrollPosition={$activeTab.scrollPosition}
                onChange={handleContentChange}
                onCursorChange={handleCursorChange}
                onScrollSync={handleEditorScroll}
                onSessionChange={handleEditorSession}
              />
            {/key}
          </section>
        {/if}

        {#if $activeTab && $uiStore.layoutMode === 'split'}
          <SplitPaneSeparator
            ratio={$uiStore.splitRatio}
            onRatioChange={uiActions.setSplitRatio}
            onCommit={uiActions.commitSplitRatio}
            onCollapse={uiActions.setLayoutMode}
          />
        {/if}

        {#if $activeTab && ($uiStore.layoutMode === 'preview' || $uiStore.layoutMode === 'split')}
          <PreviewPane
            bind:this={previewRef}
            html={$activeTab.html}
            settings={$settingsStore}
            documentPath={$activeTab.path}
            onOpenDocument={openPreviewDocument}
          />
        {/if}
      {/if}
    </main>
  </div>

  {#if $settingsStore.showStatusBar}
    <StatusBar tab={$activeTab} layoutMode={$uiStore.layoutMode} />
  {/if}
</div>

<SettingsDialog
  open={$uiStore.settingsOpen}
  settings={$settingsStore}
  onSave={saveSettings}
  onReset={resetSettings}
  onClose={uiActions.closeSettings}
/>

<CommandPalette open={$uiStore.commandPaletteOpen} commands={commandItems} onClose={uiActions.closeCommandPalette} />
<AboutDialog
  open={$uiStore.aboutOpen}
  onClose={uiActions.closeAbout}
  onExportDiagnostics={() => void exportStartupDiagnostics()}
  onClearDiagnostics={() => void clearStartupDiagnostics()}
/>

<div class="toast-stack">
  {#each $uiStore.toasts as toast}
    <button type="button" class={`toast ${toast.tone}`} on:click={() => uiActions.dismissToast(toast.id)}>
      {toast.message}
    </button>
  {/each}
</div>
