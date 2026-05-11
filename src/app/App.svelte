<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { get } from 'svelte/store';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import MarkdownEditor from '../components/editor/MarkdownEditor.svelte';
  import PreviewPane from '../components/editor/PreviewPane.svelte';
  import Sidebar from '../components/layout/Sidebar.svelte';
  import StatusBar from '../components/layout/StatusBar.svelte';
  import TitleBar from '../components/layout/TitleBar.svelte';
  import CommandPalette from '../components/layout/CommandPalette.svelte';
  import SettingsDialog from '../components/dialogs/SettingsDialog.svelte';
  import AboutDialog from '../components/dialogs/AboutDialog.svelte';
  import { activeTab, documentStore, getActiveTab, type CursorPosition, type EditorTab } from './stores/documentStore';
  import { settingsStore } from './stores/settingsStore';
  import { uiActions, uiStore, type LayoutMode } from './stores/uiStore';
  import {
    api,
    confirmAction,
    isTauriRuntime,
    pickHtmlSavePath,
    pickMarkdownFile,
    pickMarkdownSavePath,
    toAppError,
    type AppSettings,
    type RecentFileDto
  } from '../lib/tauriApi';
  import { toolbarItems, type ToolbarAction } from '../lib/markdownToolbar';
  import type { CommandItem } from '../lib/commands';

  let editorRef: any;
  let recentFiles: RecentFileDto[] = [];
  let renderTimer: number | undefined;
  let autosaveTimer: number | undefined;
  let autosaveKey = '';
  let lastRenderKey = '';
  let unlistenDrop: (() => void) | undefined;

  $: currentTitle = $activeTab?.title ?? 'Untitled.md';
  $: effectiveSidebarVisible = $uiStore.sidebarVisible && $settingsStore.showSidebar;
  $: commandItems = buildCommands();

  $: if ($settingsStore) {
    applyTheme($settingsStore);
  }

  $: if ($activeTab && $settingsStore.livePreviewEnabled) {
    const renderKey = `${$activeTab.id}:${$activeTab.content}`;
    if (renderKey !== lastRenderKey) {
      lastRenderKey = renderKey;
      scheduleRender($activeTab.id, $activeTab.content);
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
          if (tab?.path && tab.isDirty) {
            void saveActive(false);
          }
        }, Math.max(1000, $settingsStore.autosaveIntervalMs));
      }
    }
  }

  onMount(() => {
    void initialize();

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
    };
  });

  onDestroy(() => {
    if (renderTimer) window.clearTimeout(renderTimer);
    if (autosaveTimer) window.clearInterval(autosaveTimer);
  });

  async function initialize() {
    await settingsStore.load();
    const settings = get(settingsStore);
    uiActions.setSidebarVisible(settings.showSidebar);
    await refreshRecentFiles();
    await openStartupFile();
    await renderActiveNow();
    await setupDragDrop();
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

  function scheduleRender(tabId: string, content: string) {
    if (renderTimer) window.clearTimeout(renderTimer);
    renderTimer = window.setTimeout(() => {
      void renderTab(tabId, content);
    }, Math.max(100, $settingsStore.previewDebounceMs));
  }

  async function renderActiveNow() {
    const tab = getActiveTab();
    if (tab) {
      await renderTab(tab.id, tab.content);
    }
  }

  async function renderTab(tabId: string, content: string) {
    try {
      const rendered = await api.renderMarkdown(content);
      documentStore.updateRendered(tabId, rendered);
    } catch (error) {
      uiActions.toast(toAppError(error).message, 'error');
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

  async function openPath(path: string) {
    try {
      const document = await api.openMarkdownFile(path);
      documentStore.openDocument(document);
      await refreshRecentFiles();
      uiActions.toast(`已打开 ${document.title}`);
    } catch (error) {
      uiActions.toast(toAppError(error).message, 'error');
    }
  }

  async function saveActive(showToast = true) {
    const tab = getActiveTab();
    if (!tab) return;

    try {
      const path = tab.path ?? (await pickMarkdownSavePath(tab.title));
      if (!path) return;

      const saved = await api.saveMarkdownFile(path, tab.content);
      documentStore.markSaved(saved);
      await refreshRecentFiles();

      if (showToast) {
        uiActions.toast(`已保存 ${saved.title}`);
      }
    } catch (error) {
      uiActions.toast(toAppError(error).message, 'error');
    }
  }

  async function saveActiveAs() {
    const tab = getActiveTab();
    if (!tab) return;

    try {
      const path = await pickMarkdownSavePath(tab.path ?? tab.title);
      if (!path) return;
      const saved = await api.saveMarkdownFile(path, tab.content);
      documentStore.markSaved(saved);
      await refreshRecentFiles();
      uiActions.toast(`已保存 ${saved.title}`);
    } catch (error) {
      uiActions.toast(toAppError(error).message, 'error');
    }
  }

  async function exportHtml() {
    const tab = getActiveTab();
    if (!tab) return;

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

  async function closeTab(tab: EditorTab) {
    if (tab.isDirty) {
      const confirmed = await confirmAction(`“${tab.title}” 尚未保存，确定关闭？`, '关闭未保存文件');
      if (!confirmed) return;
    }
    documentStore.closeTab(tab.id);
  }

  function setLayoutMode(mode: LayoutMode) {
    uiActions.setLayoutMode(mode);
  }

  function handleContentChange(content: string) {
    documentStore.updateActiveContent(content);
  }

  function handleCursorChange(position: CursorPosition) {
    documentStore.updateActiveCursor(position);
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

  async function saveSettings(settings: AppSettings) {
    await settingsStore.save(settings);
    uiActions.setSidebarVisible(settings.showSidebar);
    uiActions.closeSettings();
  }

  function resetSettings() {
    void settingsStore.reset();
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
    } else if (key === 'f' || key === 'h') {
      event.preventDefault();
      editorRef?.openFind();
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
      { id: 'about', title: '关于 MarkLite', category: '帮助', action: uiActions.openAbout }
    ];
  }
</script>

<svelte:head>
  <title>{currentTitle} - MarkLite</title>
</svelte:head>

<div class="app-shell">
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

  <div class="tabbar">
    {#each $documentStore.tabs as tab}
      <button type="button" class:active={tab.id === $documentStore.activeTabId} on:click={() => documentStore.setActive(tab.id)}>
        <span class:dirty-dot={tab.isDirty}></span>
        {tab.title}
        <span
          class="tab-close"
          role="button"
          tabindex="0"
          on:click|stopPropagation={() => void closeTab(tab)}
          on:keydown|stopPropagation={(event) => {
            if (event.key === 'Enter' || event.key === ' ') void closeTab(tab);
          }}
        >
          ×
        </span>
      </button>
    {/each}
  </div>

  {#if $settingsStore.markdownToolbarEnabled && $uiStore.layoutMode !== 'preview'}
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
        onJumpToLine={jumpToLine}
      />
    {/if}

    <main class={`editor-stage layout-${$uiStore.layoutMode}`}>
      {#if $activeTab && ($uiStore.layoutMode === 'edit' || $uiStore.layoutMode === 'split')}
        <section class="editor-pane">
          {#key $activeTab.id}
            <MarkdownEditor
              bind:this={editorRef}
              value={$activeTab.content}
              settings={$settingsStore}
              onChange={handleContentChange}
              onCursorChange={handleCursorChange}
            />
          {/key}
        </section>
      {/if}

      {#if $activeTab && ($uiStore.layoutMode === 'preview' || $uiStore.layoutMode === 'split')}
        <PreviewPane html={$activeTab.html} settings={$settingsStore} />
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
<AboutDialog open={$uiStore.aboutOpen} onClose={uiActions.closeAbout} />

<div class="toast-stack">
  {#each $uiStore.toasts as toast}
    <button type="button" class={`toast ${toast.tone}`} on:click={() => uiActions.dismissToast(toast.id)}>
      {toast.message}
    </button>
  {/each}
</div>
