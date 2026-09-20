<script lang="ts">
  import { onDestroy, onMount, tick } from 'svelte';
  import { get } from 'svelte/store';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import type { DragDropEvent } from '@tauri-apps/api/webview';
  import Sidebar from '../components/layout/Sidebar.svelte';
  import DocumentTabBar from '../components/layout/DocumentTabBar.svelte';
  import SplitPaneSeparator from '../components/layout/SplitPaneSeparator.svelte';
  import SidebarResizeSeparator from '../components/layout/SidebarResizeSeparator.svelte';
  import StatusBar from '../components/layout/StatusBar.svelte';
  import TitleBar from '../components/layout/TitleBar.svelte';
  import {
    activeEditor,
    activePreview,
    activeRender,
    activeShell,
    activeSidebar,
    activeStatus,
    documentStore,
    getActiveTab,
    sessionProjection,
    tabBarState,
    type CursorPosition,
    type DocumentState,
    type EditorScrollPosition,
    type EditorTab
  } from './stores/documentStore';
  import { settingsOperationBusy, settingsStore } from './stores/settingsStore';
  import { uiActions, uiStore, type LayoutMode } from './stores/uiStore';
  import {
    api,
    confirmAction,
    isTauriRuntime,
    pickExportSavePath,
    pickExportParentDirectory,
    pickMarkdownFile,
    pickMarkdownSavePath,
    pickStartupDiagnosticsSavePath,
    openExternalLink,
    toAppError,
    type AppSettings,
    type DocumentOperationDto,
    type ExportFormat,
    type ExportOptions,
    type RecentFileDto,
    type SessionStateDto
  } from '../lib/tauriApi';
  import { toolbarItems, type ToolbarAction } from '../lib/markdownToolbar';
  import {
    buildCommandItems,
    findKeyboardCommand,
    type CommandActions,
    type CommandId,
    type CommandItem
  } from '../lib/commands';
  import { createAsyncSingleFlight } from '../lib/asyncSingleFlight';
  import { saveDocumentSnapshot } from '../lib/documentSave';
  import { isSameFileIdentity, isSameFilePath } from '../lib/filePathIdentity';
  import {
    createExitProtectionController,
    type DirtyExitDocument,
    type ExitPromptState
  } from '../lib/exitProtection';
  import { startupElapsedMs, type FrontendStartupStage } from '../lib/startupLifecycle';
  import { ExportProgressTask, type ExportProgressView } from '../lib/exportProgress';
  import { runExportJob, createExportJobId } from '../lib/documentExport';
  import { exportWarningReport, type ExportWarningReport } from '../lib/exportWarningReport';
  import { appInitialization } from '../lib/appInitialization';
  import { openDocumentPath } from '../lib/documentOpen';
  import { createLifecycleScope } from '../lib/lifecycleScope';
  import { shouldHandleGlobalShortcut } from '../lib/shortcutGuard';
  import { observeMediaQuery } from '../lib/mediaQuery';
  import { createMarkdownRenderCoordinator, type MarkdownRenderRequest } from '../lib/markdownRenderCoordinator';
  import { paneVisibility } from '../lib/layoutVisibility';
  import { createAutosaveScheduler } from '../lib/autosaveScheduler';
  import { createLazyComponent } from '../lib/lazyComponent';
  import { createSessionCoordinator } from '../lib/sessionCoordinator';
  import { serializeMindMapSvg } from '../lib/mindMapSvg';
  import { localizeError, t, translator } from '../lib/i18n';

  type MarkdownEditorHandle = {
    applyMarkdown: (action: ToolbarAction) => void;
    jumpToLine: (line: number) => void;
    scrollToLine: (line: number) => void;
    openFind: () => void;
    openReplace: () => void;
    flushContent: () => void;
  };

  type PreviewPaneHandle = {
    scrollToFragment: (fragment: string) => void;
    syncToEditorScroll: (position: EditorScrollPosition | undefined, identity: { tabId: string; contentRevision: number }) => void;
  };

  type MarkdownEditorProps = {
    tabId: string;
    value: string;
    settings: AppSettings;
    serializedState: import('../lib/editorSession').SerializedEditorState | null;
    initialScrollPosition: EditorScrollPosition;
    onChange: (tabId: string, value: string, lineCount: number) => void;
    onDirty: (tabId: string) => void;
    onCursorChange: (tabId: string, position: CursorPosition) => void;
    onScrollSync: (tabId: string, position: EditorScrollPosition, userInitiated: boolean) => void;
    onSessionChange: (tabId: string, state: import('../lib/editorSession').SerializedEditorState) => void;
  };

  type PreviewPaneProps = {
    html: import('../lib/tauriApi').SanitizedMarkdownHtml;
    tabId: string;
    contentRevision: number;
    renderedRevision: number;
    sourceBlocks: import('../lib/tauriApi').SourceBlock[];
    virtualPreview: import('../lib/tauriApi').VirtualPreviewIndex | null;
    diagrams: import('../lib/tauriApi').DiagramSource[];
    diagramDiagnostics: import('../lib/tauriApi').DiagramDiagnostic[];
    settings: AppSettings;
    documentPath: string | null;
    documentTitle: string;
    outline: import('../lib/tauriApi').OutlineItem[];
    onOpenDocument: (path: string, fragment: string | null) => Promise<boolean>;
    onJumpToLine: (line: number) => void;
    onSyncEditorLine: (line: number) => void;
    onCopySource: () => Promise<void>;
  };

  const markdownEditorLoader = createLazyComponent<MarkdownEditorProps, MarkdownEditorHandle>(
    () => import('../components/editor/MarkdownEditor.svelte')
  );
  const previewPaneLoader = createLazyComponent<PreviewPaneProps, PreviewPaneHandle>(
    () => import('../components/editor/PreviewPane.svelte')
  );
  const settingsDialogLoader = createLazyComponent(() => import('../components/dialogs/SettingsDialog.svelte'));
  const commandPaletteLoader = createLazyComponent(() => import('../components/layout/CommandPalette.svelte'));
  const exportDialogLoader = createLazyComponent(() => import('../components/dialogs/ExportDialog.svelte'));
  const exportWarningsLoader = createLazyComponent(() => import('../components/dialogs/ExportWarningsDialog.svelte'));
  const aboutDialogLoader = createLazyComponent(() => import('../components/dialogs/AboutDialog.svelte'));
  const markdownGuideDialogLoader = createLazyComponent(() => import('../components/dialogs/MarkdownGuideDialog.svelte'));
  const exitDialogLoader = createLazyComponent(() => import('../components/dialogs/ExitConfirmationDialog.svelte'));
  const conflictDialogLoader = createLazyComponent(() => import('../components/dialogs/ExternalChangeDialog.svelte'));
  const failedLazyComponents = new Set<string>();

  function errorMessage(error: unknown): string {
    return localizeError(toAppError(error));
  }

  let MarkdownEditor = markdownEditorLoader.current();
  let PreviewPane = previewPaneLoader.current();
  let SettingsDialog = settingsDialogLoader.current();
  let CommandPalette = commandPaletteLoader.current();
  let ExportDialog = exportDialogLoader.current();
  let ExportWarningsDialog = exportWarningsLoader.current();
  let AboutDialog = aboutDialogLoader.current();
  let MarkdownGuideDialog = markdownGuideDialogLoader.current();
  let ExitConfirmationDialog = exitDialogLoader.current();
  let ExternalChangeDialog = conflictDialogLoader.current();

  let editorRef: MarkdownEditorHandle | undefined;
  let previewRef: PreviewPaneHandle | undefined;
  let recentFiles: RecentFileDto[] = [];
  let autosaveTimer: number | undefined;
  let autosaveKey = '';
  let lastSessionKey = '';
  let sessionInitialized = false;
  let pendingPreviewFragment: { tabId: string; fragment: string } | null = null;
  let previewFragmentTimer: number | undefined;
  let openRequestDrainRunning = false;
  let openRequestDrainRequested = false;
  const saveSingleFlight = createAsyncSingleFlight<string, DocumentOperationDto | null>();
  const loadSingleFlight = createAsyncSingleFlight<string, EditorTab | null>();
  const autosaveFailureKeys = new Map<string, string>();
  const autosaveConflictTabs = new Set<string>();
  const autosaveScheduler = createAutosaveScheduler(
    () => get(documentStore).tabs.map((tab) => ({
      id: tab.id,
      path: tab.path,
      dirty: tab.isDirty,
      loaded: tab.loadState === 'loaded',
      blocked: autosaveConflictTabs.has(tab.id)
    })),
    (tabId, path) => saveTab(tabId, path, false)
  );
  const lifecycleScope = createLifecycleScope();
  const sessionCoordinator = createSessionCoordinator({
    persist: async ({ enabled, session }) => {
      if (enabled) await api.updateSession(session);
      else await api.clearSession();
    },
    onError: (error) => uiActions.toast(errorMessage(error), 'error')
  });
  let allowBrowserUnload = false;
  let exitPrompt: ExitPromptState | null = null;
  let saveConflictPrompt: { tabId: string; path: string; title: string; busy: boolean } | null = null;
  let exportDialogOpen = false;
  let exportBusy = false;
  let exportProgress: ExportProgressView | null = null;
  let exportTask: ExportProgressTask | null = null;
  let imageExportJob: { id: string; executing: boolean; cancelRequested: boolean } | null = null;
  let lastExportWarnings: ExportWarningReport | null = null;
  let warningDetailsOpen = false;
  let initializationComplete = false;
  const narrowViewportQuery = window.matchMedia?.('(max-width: 920px)');
  let narrowViewport = narrowViewportQuery?.matches ?? false;
  const initializationStartedAt = performance.now();
  const markdownCoordinator = createMarkdownRenderCoordinator({
    render: (content, tabId, contentRevision) => api.renderMarkdown(content, tabId, contentRevision),
    analyze: (content) => api.analyzeMarkdown(content),
    onRendered: applyRenderedMarkdown,
    onAnalyzed: (request, analysis) => documentStore.updateAnalysis(request.tabId, request.contentRevision, analysis),
    onError: (request, error) => {
      if (documentStore.isCurrentRevision(request.tabId, request.contentRevision)) {
        uiActions.toast(errorMessage(error), 'error');
      }
    }
  });
  const exitProtection = createExitProtectionController({
    getDirtyDocuments: getDirtyExitDocuments,
    onPromptChange: (state) => {
      if (state) {
        uiActions.closeModals();
        exportDialogOpen = false;
      }
      exitPrompt = state;
    },
    saveDocument: saveDirtyDocumentBeforeExit,
    closeWindow: closeApplicationWindow,
    onCloseError: (error) => {
      uiActions.toast(t('toast.exitFailed', { message: errorMessage(error) }), 'error');
    }
  });

  $: currentTitle = $activeShell?.title ?? 'Untitled.md';
  $: effectiveSidebarVisible = $uiStore.sidebarVisible;
  $: visiblePanes = paneVisibility($uiStore.layoutMode, narrowViewport);
  $: commandItems = buildCommands();
  $: if (initializationComplete && $activeEditor?.loadState === 'loaded' && visiblePanes.editor) {
    void loadMarkdownEditor();
  }
  $: if (initializationComplete && $activePreview?.loadState === 'loaded' && visiblePanes.preview) {
    void loadPreviewPane();
  }
  $: if ($uiStore.settingsOpen) void loadSettingsDialog();
  $: if ($uiStore.commandPaletteOpen) void loadCommandPalette();
  $: if (exportDialogOpen) void loadExportDialog();
  $: if (warningDetailsOpen) void loadExportWarningsDialog();
  $: if ($uiStore.aboutOpen) void loadAboutDialog();
  $: if ($uiStore.markdownGuideOpen) void loadMarkdownGuideDialog();
  $: if (exitPrompt) void loadExitDialog();
  $: if (saveConflictPrompt) void loadConflictDialog();

  $: if ($settingsStore) {
    applyTheme($settingsStore);
    documentStore.localizeWelcome();
  }

  $: {
    const active = initializationComplete && $activeRender?.loadState === 'loaded' ? $activeRender : null;
    const renderVisible = Boolean(active && $settingsStore.livePreviewEnabled && visiblePanes.preview);
    markdownCoordinator.update(
      active ? { tabId: active.id, content: active.content, contentRevision: active.contentRevision } : null,
      renderVisible ? 'render' : 'analysis',
      active ? documentStore.getTab(active.id)?.analysisRevision === active.contentRevision : false,
      $settingsStore.previewDebounceMs
    );
  }

  $: {
    const nextKey = `${$settingsStore.autosaveEnabled}:${$settingsStore.autosaveIntervalMs}`;
    if (nextKey !== autosaveKey) {
      autosaveKey = nextKey;
      if (autosaveTimer) window.clearInterval(autosaveTimer);
      autosaveTimer = undefined;

      if ($settingsStore.autosaveEnabled) {
        autosaveTimer = window.setInterval(() => {
          void autosaveScheduler.run();
        }, Math.max(1000, $settingsStore.autosaveIntervalMs));
      }
    }
  }

  $: if (sessionInitialized) {
    const session: SessionStateDto = { version: 1, ...$sessionProjection };
    const sessionKey = `${$settingsStore.restoreLastSession}:${session.activePath ?? ''}:${session.paths.join('\u0000')}`;
    if (sessionKey !== lastSessionKey) {
      lastSessionKey = sessionKey;
      sessionCoordinator.queue({ enabled: $settingsStore.restoreLastSession, session });
    }
  }

  async function loadMarkdownEditor() {
    if (!MarkdownEditor) {
      await loadLazyComponent('editor', t('lazy.editor'), markdownEditorLoader.load, (component) => {
        MarkdownEditor = component;
      });
    }
  }

  async function loadPreviewPane() {
    if (!PreviewPane) {
      await loadLazyComponent('preview', t('lazy.preview'), previewPaneLoader.load, (component) => {
        PreviewPane = component;
      });
    }
  }

  async function loadSettingsDialog() {
    if (!SettingsDialog) {
      await loadLazyComponent('settings', t('lazy.settings'), settingsDialogLoader.load, (component) => {
        SettingsDialog = component;
      });
    }
  }

  async function loadCommandPalette() {
    if (!CommandPalette) {
      await loadLazyComponent('command-palette', t('lazy.commandPalette'), commandPaletteLoader.load, (component) => {
        CommandPalette = component;
      });
    }
  }

  async function loadExportDialog() {
    if (!ExportDialog) {
      await loadLazyComponent('export-dialog', t('lazy.exportDialog'), exportDialogLoader.load, (component) => {
        ExportDialog = component;
      });
    }
  }

  async function loadExportWarningsDialog() {
    if (!ExportWarningsDialog) {
      await loadLazyComponent('export-warnings', t('export.warningTitle'), exportWarningsLoader.load, (component) => {
        ExportWarningsDialog = component;
      });
    }
  }

  async function loadAboutDialog() {
    if (!AboutDialog) {
      await loadLazyComponent('about-dialog', t('lazy.aboutDialog'), aboutDialogLoader.load, (component) => {
        AboutDialog = component;
      });
    }
  }

  async function loadMarkdownGuideDialog() {
    if (!MarkdownGuideDialog) {
      await loadLazyComponent('markdown-guide', t('lazy.markdownGuide'), markdownGuideDialogLoader.load, (component) => {
        MarkdownGuideDialog = component;
      });
    }
  }

  async function loadExitDialog() {
    if (!ExitConfirmationDialog) {
      await loadLazyComponent('exit-dialog', t('lazy.exitDialog'), exitDialogLoader.load, (component) => {
        ExitConfirmationDialog = component;
      });
    }
  }

  async function loadConflictDialog() {
    if (!ExternalChangeDialog) {
      await loadLazyComponent('conflict-dialog', t('lazy.conflictDialog'), conflictDialogLoader.load, (component) => {
        ExternalChangeDialog = component;
      });
    }
  }

  async function loadLazyComponent<T>(
    key: string,
    name: string,
    load: () => Promise<T>,
    assign: (component: T) => void
  ): Promise<void> {
    if (failedLazyComponents.has(key)) return;
    try {
      assign(await load());
    } catch (error) {
      failedLazyComponents.add(key);
      uiActions.toast(t('toast.componentLoadFailed', { name, message: errorMessage(error) }), 'error');
    }
  }

  onMount(() => {
    void initialize()
      .then(async () => {
        initializationComplete = true;
        await tick();
        appInitialization.succeed();
      })
      .catch((error) => {
        appInitialization.fail();
        uiActions.toast(t('toast.initializationIncomplete', { message: errorMessage(error) }), 'error');
      });
    if (isTauriRuntime()) {
      void setupCloseProtection();
    }

    const keyHandler = (event: KeyboardEvent) => {
      handleGlobalShortcut(event);
    };
    const beforeUnload = (event: BeforeUnloadEvent) => {
      if (!allowBrowserUnload && get(documentStore).tabs.some((tab) => tab.isDirty)) {
        event.preventDefault();
        event.returnValue = '';
      }
    };

    window.addEventListener('keydown', keyHandler);
    window.addEventListener('beforeunload', beforeUnload);
    lifecycleScope.own(() => window.removeEventListener('keydown', keyHandler));
    lifecycleScope.own(() => window.removeEventListener('beforeunload', beforeUnload));
    if (window.matchMedia) {
      lifecycleScope.own(
        observeMediaQuery(window.matchMedia('(prefers-color-scheme: dark)'), () => {
          const settings = get(settingsStore);
          if (settings.theme === 'system') applyTheme(settings);
        })
      );
    }
    if (narrowViewportQuery) {
      lifecycleScope.own(observeMediaQuery(narrowViewportQuery, (matches) => (narrowViewport = matches)));
    }

    return () => {
      const cleanupErrors = lifecycleScope.dispose();
      if (cleanupErrors.length > 0) {
        console.warn(`Failed to release ${cleanupErrors.length} application resources`);
      }
    };
  });

  onDestroy(() => {
    if (imageExportJob) void cancelImageExport();
    exportTask?.dispose();
    markdownCoordinator.dispose();
    autosaveScheduler.dispose();
    if (previewFragmentTimer !== undefined) window.clearTimeout(previewFragmentTimer);
    if (autosaveTimer) window.clearInterval(autosaveTimer);
    sessionCoordinator.dispose();
  });

  async function initialize() {
    recordStartupStage('initialization', 'started');
    try {
      await runStartupStage('settings', async () => {
        const loaded = await settingsStore.load();
        const settings = get(settingsStore);
        uiActions.setSidebarVisible(settings.showSidebar);
        return loaded;
      });
      const settings = get(settingsStore);
      await runStartupStage('recentFiles', refreshRecentFiles);
      const sessionWritable = await runStartupStage('sessionRestore', () => restoreSession(settings.restoreLastSession));
      sessionCoordinator.setWritable(sessionWritable);
      sessionInitialized = true;
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

  async function runStartupStage(stage: FrontendStartupStage, operation: () => Promise<void | boolean>) {
    recordStartupStage(stage, 'started');
    try {
      const result = await operation();
      recordStartupStage(
        stage,
        result === false ? 'degraded' : 'succeeded',
        result === false ? 'stageDegraded' : null
      );
      return result !== false;
    } catch (error) {
      recordStartupStage(stage, 'failed', 'initializationFailed');
      throw error;
    }
  }

  function recordStartupStage(
    stage: FrontendStartupStage,
    status: 'started' | 'succeeded' | 'degraded' | 'failed',
    code: 'initializationFailed' | 'stageDegraded' | null = null
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
        return true;
      } catch (error) {
        uiActions.toast(errorMessage(error), 'error');
        return false;
      }
    }

    try {
      const session = await api.getSession();
      documentStore.restoreSessionTabs(session.paths.slice(0, 50), session.activePath);
      return true;
    } catch (error) {
      uiActions.toast(errorMessage(error), 'error');
      return false;
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

  async function openStartupFile() {
    try {
      const path = await api.getStartupFileArg();
      if (path) {
        return Boolean(await openPath(path));
      }
      return true;
    } catch (error) {
      console.warn('No startup file argument available', error);
      return false;
    }
  }

  async function setupDragDrop() {
    if (!isTauriRuntime()) return true;

    try {
      const unlisten = await getCurrentWindow().onDragDropEvent((event: { payload: DragDropEvent }) => {
        if (event.payload?.type !== 'drop') return;
        const path = event.payload.paths.find((item) => /\.(md|markdown|txt)$/i.test(item));
        if (path) {
          void openPath(path);
        }
      });
      lifecycleScope.own(unlisten);
      return true;
    } catch (error) {
      console.warn('Failed to register drag drop handler', error);
      return false;
    }
  }

  async function setupExternalOpenListener() {
    if (!isTauriRuntime()) return true;

    try {
      const unlisten = await getCurrentWindow().listen('single-instance-open-file', () => {
        void requestOpenFileDrain().catch((error) => {
          uiActions.toast(errorMessage(error), 'error');
        });
      });
      lifecycleScope.own(unlisten);
      await requestOpenFileDrain();
      return true;
    } catch (error) {
      console.warn('Failed to register external open listener', error);
      return false;
    }
  }

  async function requestOpenFileDrain() {
    openRequestDrainRequested = true;
    if (openRequestDrainRunning) return;
    openRequestDrainRunning = true;
    try {
      do {
        openRequestDrainRequested = false;
        const paths = await api.drainOpenFileRequests();
        for (const path of paths) {
          await openPath(path);
        }
      } while (openRequestDrainRequested);
    } finally {
      openRequestDrainRunning = false;
    }
  }

  async function setupCloseProtection() {
    try {
      const unlisten = await getCurrentWindow().onCloseRequested((event) => {
        flushActiveEditor();
        exitProtection.handleCloseRequest(() => event.preventDefault());
      });
      lifecycleScope.own(unlisten);
    } catch (error) {
      console.warn('Failed to register close protection', error);
      uiActions.toast(t('toast.nativeExitUnavailable'), 'error');
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
  }

  async function renderActiveNow() {
    const tab = getActiveTab();
    if (tab?.loadState === 'loaded') {
      const useFullRender = $settingsStore.livePreviewEnabled && visiblePanes.preview;
      return markdownCoordinator.runNow({
        kind: useFullRender ? 'render' : 'analysis',
        tabId: tab.id,
        content: tab.content,
        contentRevision: tab.contentRevision
      });
    }
    return true;
  }

  async function copyPreviewSource() {
    flushActiveEditor();
    const tab = getActiveTab();
    if (!tab) return;
    try {
      await navigator.clipboard.writeText(tab.content);
    } catch (error) {
      uiActions.toast(errorMessage(error), 'error');
    }
  }

  function applyRenderedMarkdown(request: MarkdownRenderRequest, rendered: import('../lib/tauriApi').RenderedMarkdownDto) {
    const { tabId, contentRevision } = request;
    if (rendered.virtualPreview && tabId !== getActiveTab()?.id) {
      void api.releaseMarkdownPreview(rendered.virtualPreview.sessionId);
      return false;
    }
    const applied = documentStore.updateRendered(tabId, contentRevision, rendered);
    if (!applied && rendered.virtualPreview) void api.releaseMarkdownPreview(rendered.virtualPreview.sessionId);
    let openedFragment = false;
    if (applied && pendingPreviewFragment?.tabId === tabId) {
      const fragment = pendingPreviewFragment.fragment;
      pendingPreviewFragment = null;
      openedFragment = true;
      if (previewFragmentTimer !== undefined) window.clearTimeout(previewFragmentTimer);
      previewFragmentTimer = window.setTimeout(() => {
        previewFragmentTimer = undefined;
        previewRef?.scrollToFragment(fragment);
      }, 0);
    }
    if (applied && !openedFragment && tabId === getActiveTab()?.id && $settingsStore.syncScroll) {
      const position = getActiveTab()?.scrollPosition;
      previewRef?.syncToEditorScroll(position, { tabId, contentRevision });
    }
    return applied;
  }

  async function refreshRecentFiles() {
    try {
      recentFiles = await api.getRecentFiles();
      return true;
    } catch (error) {
      uiActions.toast(errorMessage(error), 'error');
      return false;
    }
  }

  function newDocument() {
    flushActiveEditor();
    documentStore.newDocument();
    uiActions.toast(t('toast.newDocument'));
  }

  async function openFile() {
    try {
      const path = await pickMarkdownFile();
      if (path) {
        await openPath(path);
      }
    } catch (error) {
      uiActions.toast(errorMessage(error), 'error');
    }
  }

  async function openPath(path: string): Promise<EditorTab | null> {
    flushActiveEditor();
    try {
      const { tab, result } = await openDocumentPath(
        path,
        api.openMarkdownFile,
        documentStore.openDocument,
        refreshRecentFiles
      );
      const auxiliaryError = result.auxiliaryError;
      if (auxiliaryError) {
        uiActions.toast(t('toast.openedRecentFailed', {
          title: result.document.title,
          message: localizeError(auxiliaryError)
        }), 'error');
      } else {
        uiActions.toast(t('toast.opened', { title: result.document.title }));
      }
      return tab;
    } catch (error) {
      uiActions.toast(errorMessage(error), 'error');
      return null;
    }
  }

  async function activateTab(tabId: string) {
    flushActiveEditor();
    documentStore.setActive(tabId);
    await loadTabContent(tabId);
  }

  async function loadTabContent(tabId: string): Promise<EditorTab | null> {
    const request = loadSingleFlight.run(tabId, async () => {
      const tab = documentStore.getTab(tabId);
      if (!tab) return null;
      if (tab.loadState === 'loaded') return tab;
      if (!tab.path) {
        documentStore.markLoadFailed(tabId, t('app.noReadablePath'));
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
          uiActions.toast(t('toast.loadedRecentFailed', {
            title: result.document.title,
            message: localizeError(result.auxiliaryError)
          }), 'error');
        }
        return documentStore.getTab(tabId) ?? null;
      } catch (error) {
        const appError = toAppError(error);
        const message = localizeError(appError);
        documentStore.markLoadFailed(tabId, message);
        uiActions.toast(message, 'error');
        return null;
      }
    });
    return request.promise;
  }

  async function openPreviewDocument(path: string, fragment: string | null): Promise<boolean> {
    const tab = await openPath(path);
    if (!tab) return false;
    if (!fragment) return true;
    pendingPreviewFragment = { tabId: tab.id, fragment };
    if (tab.renderedRevision === tab.contentRevision) {
      pendingPreviewFragment = null;
      window.setTimeout(() => previewRef?.scrollToFragment(fragment), 0);
    }
    return true;
  }

  async function saveActive(showToast = true) {
    flushActiveEditor();
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
      uiActions.toast(errorMessage(error), 'error');
    }
  }

  async function saveActiveAs() {
    flushActiveEditor();
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
      uiActions.toast(errorMessage(error), 'error');
    }
  }

  async function saveTab(
    tabId: string,
    path: string,
    showToast: boolean,
    overwriteExternalChanges = false
  ): Promise<boolean> {
    if (tabId === getActiveTab()?.id) flushActiveEditor();
    const request = saveSingleFlight.run(tabId, async () => {
      const result = await saveDocumentSnapshot(tabId, path, {
        getTab: documentStore.getTab,
        resolveFileVersion: api.resolveFileVersion,
        getFileOwner: documentStore.getFileOwner,
        activateTab: documentStore.setActive,
        saveFile: api.saveMarkdownFile,
        markSaved: documentStore.markSaved
      }, overwriteExternalChanges);
      if (!result) return null;
      await refreshRecentFiles();
      return result;
    });

    try {
      const result = await request.promise;
      if (!result) return false;
      const saved = result.document;

      const current = documentStore.getTab(tabId);
      if (!request.started && current) {
        const requestedVersion = await api.resolveFileVersion(path, true);
        const matchesRequestedTarget = requestedVersion
          ? isSameFileIdentity(current.fileIdentity, requestedVersion.fileIdentity)
          : isSameFilePath(current.path, path);
        if (current.isDirty || !matchesRequestedTarget) {
          return saveTab(tabId, path, showToast);
        }
      }

      const savedMessage = current?.isDirty
        ? t('toast.savedSnapshot', { title: saved.title })
        : t('toast.saved', { title: saved.title });
      if (result.auxiliaryError) {
        const failureKey = `${result.auxiliaryError.code}:${result.auxiliaryError.message}`;
        if (showToast || autosaveFailureKeys.get(tabId) !== failureKey) {
          uiActions.toast(t('toast.savedRecentFailed', {
            saved: savedMessage,
            message: localizeError(result.auxiliaryError)
          }), 'error');
        }
        if (current) autosaveFailureKeys.set(tabId, failureKey);
      } else {
        autosaveFailureKeys.delete(tabId);
        autosaveConflictTabs.delete(tabId);
        if (showToast) {
          uiActions.toast(savedMessage, current?.isDirty ? 'info' : 'success');
        }
      }
      return Boolean(
        current &&
          !current.isDirty &&
          isSameFileIdentity(current.fileIdentity, saved.fileIdentity)
      );
    } catch (error) {
      const appError = toAppError(error);
      if (appError.code === 'FILE_CONTENT_CHANGED' || appError.code === 'FILE_TARGET_CHANGED') {
        const current = documentStore.getTab(tabId);
        if (current) {
          autosaveConflictTabs.add(tabId);
          uiActions.closeModals();
          exportDialogOpen = false;
          saveConflictPrompt = { tabId, path, title: current.title, busy: false };
        }
      }
      const failureKey = `${appError.code}:${appError.message}`;
      if (showToast || autosaveFailureKeys.get(tabId) !== failureKey) {
        uiActions.toast(localizeError(appError), 'error');
      }
      if (documentStore.getTab(tabId)) autosaveFailureKeys.set(tabId, failureKey);
      return false;
    }
  }

  function closeSaveConflictPrompt(): void {
    if (!saveConflictPrompt?.busy) saveConflictPrompt = null;
  }

  async function reloadConflictedDocument(): Promise<void> {
    flushActiveEditor();
    const prompt = saveConflictPrompt;
    if (!prompt || prompt.busy) return;
    saveConflictPrompt = { ...prompt, busy: true };
    try {
      const result = await api.openMarkdownFile(prompt.path);
      if (!documentStore.reloadDocument(prompt.tabId, prompt.path, result.document)) {
        throw Object.assign(new Error('The conflicted tab changed before reload completed.'), {
          code: 'FILE_IDENTITY_CONFLICT'
        });
      }
      autosaveConflictTabs.delete(prompt.tabId);
      autosaveFailureKeys.delete(prompt.tabId);
      saveConflictPrompt = null;
      if (result.auxiliaryError) uiActions.toast(localizeError(result.auxiliaryError), 'error');
    } catch (error) {
      saveConflictPrompt = { ...prompt, busy: false };
      uiActions.toast(errorMessage(error), 'error');
    }
  }

  async function saveConflictedCopy(): Promise<void> {
    const prompt = saveConflictPrompt;
    if (!prompt || prompt.busy) return;
    try {
      const path = await pickMarkdownSavePath(prompt.title);
      if (!path) return;
      saveConflictPrompt = { ...prompt, busy: true };
      const saved = await saveTab(prompt.tabId, path, true);
      if (saved) {
        autosaveConflictTabs.delete(prompt.tabId);
        saveConflictPrompt = null;
      } else if (saveConflictPrompt?.tabId === prompt.tabId && saveConflictPrompt.busy) {
        saveConflictPrompt = { ...prompt, busy: false };
      }
    } catch (error) {
      saveConflictPrompt = { ...prompt, busy: false };
      uiActions.toast(errorMessage(error), 'error');
    }
  }

  async function overwriteConflictedDocument(): Promise<void> {
    const prompt = saveConflictPrompt;
    if (!prompt || prompt.busy) return;
    saveConflictPrompt = { ...prompt, busy: true };
    const saved = await saveTab(prompt.tabId, prompt.path, true, true);
    if (saved) {
      autosaveConflictTabs.delete(prompt.tabId);
      saveConflictPrompt = null;
    } else if (saveConflictPrompt?.tabId === prompt.tabId && saveConflictPrompt.busy) {
      saveConflictPrompt = { ...prompt, busy: false };
    }
  }

  function getDirtyExitDocuments(): DirtyExitDocument[] {
    return get(documentStore)
      .tabs.filter((tab) => tab.isDirty)
      .map((tab) => ({
        id: tab.id,
        title: tab.title,
        path: tab.path,
        contentRevision: tab.contentRevision
      }));
  }

  async function saveDirtyDocumentBeforeExit(document: DirtyExitDocument): Promise<boolean> {
    const tab = documentStore.getTab(document.id);
    if (!tab || !tab.isDirty) return true;
    if (tab.loadState !== 'loaded') {
      uiActions.toast(t('toast.unloadedCannotSave', { title: tab.title }), 'error');
      return false;
    }

    try {
      const path = tab.path ?? (await pickMarkdownSavePath(tab.title));
      if (!path) {
        uiActions.toast(t('toast.saveCancelled', { title: tab.title }), 'info');
        return false;
      }

      const saved = await saveTab(tab.id, path, true);
      if (!saved) {
        const current = documentStore.getTab(tab.id);
        if (current?.isDirty) {
          uiActions.toast(t('toast.stillDirty', { title: current.title }), 'info');
        }
      }
      return saved;
    } catch (error) {
      uiActions.toast(t('toast.saveFailed', { title: tab.title, message: errorMessage(error) }), 'error');
      return false;
    }
  }

  async function closeApplicationWindow(): Promise<void> {
    flushActiveEditor();
    sessionCoordinator.queue({
      enabled: get(settingsStore).restoreLastSession,
      session: currentSessionSnapshot(get(documentStore))
    });
    await sessionCoordinator.flush();

    allowBrowserUnload = true;
    try {
      await getCurrentWindow().destroy();
    } catch (error) {
      allowBrowserUnload = false;
      throw error;
    }

    window.setTimeout(() => {
      allowBrowserUnload = false;
    }, 500);
  }

  function openExportDialog(): void {
    flushActiveEditor();
    if (exportBusy || exitPrompt || saveConflictPrompt || !getActiveTab()) return;
    uiActions.closeModals();
    exportDialogOpen = true;
  }

  function openStoreModal(open: () => void): void {
    if (exportBusy || exitPrompt || saveConflictPrompt) return;
    exportDialogOpen = false;
    open();
  }

  const openSettingsDialogRequest = () => openStoreModal(uiActions.openSettings);
  const openCommandPaletteRequest = () => openStoreModal(uiActions.openCommandPalette);
  const openAboutDialogRequest = () => openStoreModal(uiActions.openAbout);
  const openMarkdownGuideRequest = () => openStoreModal(uiActions.openMarkdownGuide);

  function closeExportDialog(): void {
    if (!exportBusy) exportDialogOpen = false;
  }

  async function cancelImageExport() {
    const job = imageExportJob;
    if (!job || job.cancelRequested) return;
    imageExportJob = { ...job, cancelRequested: true };
    const pending = imageExportJob;
    try {
      // Registration happens in the backend. Retry only while this invocation is live.
      while (imageExportJob === pending && pending.executing) {
        if (await api.cancelPngExport(pending.id)) return;
        await new Promise((resolve) => setTimeout(resolve, 50));
      }
    } catch (error) {
      uiActions.toast(errorMessage(error), 'error');
    }
  }

  async function exportDocument(format: ExportFormat, options: ExportOptions) {
    flushActiveEditor();
    let tab = getActiveTab();
    if (!tab) return;
    exportBusy = true;
    const jobId = createExportJobId();
    const task = new ExportProgressTask(jobId, format, (view) => { exportProgress = view; });
    exportTask = task;
    if (format === 'png') imageExportJob = { id: jobId, executing: false, cancelRequested: false };
    try {
      if (isTauriRuntime()) {
        try { await task.subscribe((receive) => getCurrentWindow().listen('document-export-progress', (event) => receive(event.payload))); }
        catch (error) { console.warn('Export progress subscription unavailable', error); }
      }
      if (tab.loadState !== 'loaded') {
        const loadedTab = await loadTabContent(tab.id);
        if (!loadedTab || loadedTab.loadState !== 'loaded') return;
        tab = loadedTab;
      }
      task.phase(format === 'svg' ? 'rendering' : 'snapshot');
      const exportSource = format === 'svg'
        ? {
            ...tab,
            mindMapSvg: serializeMindMapSvg(
              tab.title,
              (await api.analyzeMarkdown(tab.content)).outline
            )
          }
        : { ...tab };
      const locationWarnings: string[] = [];
      const result = await runExportJob(
        exportSource,
        format,
        options,
        async (defaultPath) => {
          if (format !== 'png') return pickExportSavePath(format, defaultPath);
          const parent = exportSource.path ? null : await pickExportParentDirectory();
          if ((!exportSource.path && !parent) || imageExportJob?.cancelRequested) return null;
          return api.resolvePngExportDirectory(exportSource.path, exportSource.title, parent);
        },
        async (request) => {
          if (format === 'png' && imageExportJob) {
            if (imageExportJob.cancelRequested) throw { code: 'EXPORT_CANCELLED', message: 'Image export cancelled' };
            imageExportJob.executing = true;
          }
          return api.exportDocument(request);
        },
        jobId,
        {
          suggest: api.suggestExportPath,
          remember: api.rememberExportDirectory,
          onWarning: (error) => locationWarnings.push(errorMessage(error))
        },
        (stage) => task.phase(stage)
      );
      if (!result) {
        task.finish('cancelled');
        exportDialogOpen = false;
        if (locationWarnings.length) uiActions.toast(locationWarnings.join('\n'), 'error');
        return;
      }
      task.finish('succeeded');
      exportDialogOpen = false;
      lastExportWarnings = result.warnings.length ? exportWarningReport(exportSource, result) : null;
      const successMessage = result.warnings.length
        ? t('export.successWarnings', { format: format.toUpperCase(), count: result.warnings.length })
        : t('export.success', { format: format.toUpperCase() });
      uiActions.toast([successMessage, ...(format === 'png' ? [result.path] : []), ...locationWarnings].join('\n'));
    } catch (error) {
      const cancelled = toAppError(error).code === 'EXPORT_CANCELLED';
      task.finish(cancelled ? 'cancelled' : 'failed');
      uiActions.toast(errorMessage(error), cancelled ? 'info' : 'error');
    } finally {
      task.dispose();
      exportTask = null;
      exportProgress = null;
      exportBusy = false;
      imageExportJob = null;
    }
  }

  async function exportStartupDiagnostics() {
    try {
      const path = await pickStartupDiagnosticsSavePath();
      if (!path) return;
      const result = await api.exportStartupDiagnostics(path);
      uiActions.toast(t('toast.diagnosticsExported', { count: result.recordCount }));
    } catch (error) {
      uiActions.toast(errorMessage(error), 'error');
    }
  }

  async function clearStartupDiagnostics() {
    try {
      await api.clearStartupDiagnostics();
      uiActions.toast(t('toast.diagnosticsCleared'));
    } catch (error) {
      uiActions.toast(errorMessage(error), 'error');
    }
  }

  async function openRepository(url: string) {
    try {
      await openExternalLink(url);
    } catch (error) {
      uiActions.toast(errorMessage(error), 'error');
    }
  }

  async function closeTab(tab: EditorTab) {
    flushActiveEditor();
    const current = documentStore.getTab(tab.id);
    if (!current) return;
    if (current.isDirty) {
      const confirmed = await confirmAction(
        t('confirm.unsavedClose.message', { title: current.title }),
        t('confirm.unsavedClose.title')
      );
      if (!confirmed) return;
    }
    documentStore.closeTab(current.id);
    autosaveFailureKeys.delete(current.id);
  }

  async function closeTabById(tabId: string) {
    const tab = documentStore.getTab(tabId);
    if (tab) await closeTab(tab);
  }

  async function closeOtherTabs(sourceTabId: string) {
    const state = get(documentStore);
    const targets = state.tabs.filter((tab) => tab.id !== sourceTabId);
    await closeTabGroup(targets, sourceTabId, t('tabs.closeOthers'));
  }

  async function closeRightTabs(sourceTabId: string) {
    const state = get(documentStore);
    const sourceIndex = state.tabs.findIndex((tab) => tab.id === sourceTabId);
    if (sourceIndex < 0) return;
    await closeTabGroup(state.tabs.slice(sourceIndex + 1), sourceTabId, t('tabs.closeRight'));
  }

  async function closeTabGroup(tabs: EditorTab[], preferredActiveId: string, actionTitle: string) {
    flushActiveEditor();
    const currentTabs = tabs
      .map((tab) => documentStore.getTab(tab.id))
      .filter((tab): tab is EditorTab => Boolean(tab));
    if (!currentTabs.length) return;
    const dirtyTabs = currentTabs.filter((tab) => tab.isDirty);
    if (dirtyTabs.length) {
      const confirmed = await confirmAction(
        t('confirm.dirtyTabs', { count: dirtyTabs.length, action: actionTitle }),
        actionTitle
      );
      if (!confirmed) return;
    }
    documentStore.closeTabs(
      currentTabs.map((tab) => tab.id),
      preferredActiveId
    );
    for (const tab of currentTabs) autosaveFailureKeys.delete(tab.id);
  }

  function setLayoutMode(mode: LayoutMode) {
    flushActiveEditor();
    uiActions.setLayoutMode(mode);
  }

  function flushActiveEditor() {
    editorRef?.flushContent();
  }

  function handleContentChange(tabId: string, content: string, lineCount: number) {
    documentStore.updateContent(tabId, content, lineCount);
  }

  function handleContentDirty(tabId: string) {
    documentStore.markDirty(tabId);
  }

  function handleCursorChange(tabId: string, position: CursorPosition) {
    documentStore.updateCursor(tabId, position);
  }

  function handleEditorScroll(tabId: string, position: EditorScrollPosition, userInitiated: boolean) {
    documentStore.updateScroll(tabId, position);
    const active = getActiveTab();
    if (userInitiated && tabId === active?.id && $settingsStore.syncScroll && $uiStore.layoutMode === 'split') {
      previewRef?.syncToEditorScroll(position, { tabId, contentRevision: active.contentRevision });
    }
  }

  function syncEditorToPreviewLine(line: number) {
    if ($uiStore.layoutMode !== 'split') return;
    editorRef?.scrollToLine(line);
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
      uiActions.toast(errorMessage(error), 'error');
    }
  }

  async function revealRecent(path: string) {
    try {
      await api.showInFileManager(path);
    } catch (error) {
      uiActions.toast(errorMessage(error), 'error');
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
        sessionCoordinator.setWritable(true);
      } catch (error) {
        sessionCoordinator.setWritable(false);
        uiActions.toast(errorMessage(error), 'error');
        return;
      }
    }
    uiActions.setSidebarVisible(saved.showSidebar);
    uiActions.closeSettings();
    return saved;
  }

  async function resetSettings() {
    try {
      return await settingsStore.reset();
    } catch {
      return undefined;
    }
  }

  function handleGlobalShortcut(event: KeyboardEvent) {
    if (!shouldHandleGlobalShortcut(event, {
      settingsOpen: $uiStore.settingsOpen,
      commandPaletteOpen: $uiStore.commandPaletteOpen,
      aboutOpen: $uiStore.aboutOpen,
      markdownGuideOpen: $uiStore.markdownGuideOpen,
      exitConfirmationOpen: exitPrompt !== null || saveConflictPrompt !== null,
      exportDialogOpen: exportDialogOpen || warningDetailsOpen
    })) return;
    const commandId = findKeyboardCommand(event);
    if (!commandId) return;
    const action = commandActions()[commandId];
    if (!action) return;
    event.preventDefault();
    action();
  }

  function commandActions(): CommandActions {
    return {
      new: newDocument,
      open: () => void openFile(),
      save: () => void saveActive(),
      'save-as': () => void saveActiveAs(),
      'close-tab': () => {
        const tab = getActiveTab();
        if (tab) void closeTab(tab);
      },
      export: openExportDialog,
      find: () => editorRef?.openFind(),
      replace: () => editorRef?.openReplace(),
      'toggle-preview': () => {
        flushActiveEditor();
        uiActions.togglePreview();
      },
      'layout-edit': () => setLayoutMode('edit'),
      'layout-split': () => setLayoutMode('split'),
      'layout-preview': () => setLayoutMode('preview'),
      sidebar: uiActions.toggleSidebar,
      settings: openSettingsDialogRequest,
      'command-palette': openCommandPaletteRequest,
      'export-startup-diagnostics': () => void exportStartupDiagnostics(),
      'clear-startup-diagnostics': () => void clearStartupDiagnostics(),
      'markdown-guide': openMarkdownGuideRequest,
      about: openAboutDialogRequest,
      'format-bold': () => applyToolbar('bold'),
      'format-italic': () => applyToolbar('italic'),
      'insert-link': () => applyToolbar('link')
    } satisfies Partial<Record<CommandId, () => void>>;
  }

  function buildCommands(): CommandItem[] {
    return buildCommandItems(commandActions(), navigator.platform, $settingsStore.language);
  }
</script>

<svelte:head>
  <title>{currentTitle} - MarkLite</title>
</svelte:head>

<div
  class="app-shell"
  data-marklite-ready={initializationComplete ? 'true' : undefined}
  aria-label={$translator('app.ariaLabel')}
>
  <TitleBar
    title={currentTitle}
    isDirty={$activeShell?.isDirty}
    layoutMode={$uiStore.layoutMode}
    sidebarVisible={effectiveSidebarVisible}
    onNew={newDocument}
    onOpen={() => void openFile()}
    onSave={() => void saveActive()}
    onSaveAs={() => void saveActiveAs()}
    onExport={openExportDialog}
    onFind={() => editorRef?.openFind()}
    onSettings={openSettingsDialogRequest}
    onToggleSidebar={uiActions.toggleSidebar}
    onLayout={setLayoutMode}
    onCommandPalette={openCommandPaletteRequest}
    onMarkdownGuide={openMarkdownGuideRequest}
    onAbout={openAboutDialogRequest}
  />

  <DocumentTabBar
    tabs={$tabBarState.tabs}
    activeTabId={$tabBarState.activeTabId}
    onActivate={(tabId) => void activateTab(tabId)}
    onClose={(tabId) => closeTabById(tabId)}
    onCloseOthers={(tabId) => closeOtherTabs(tabId)}
    onCloseRight={(tabId) => closeRightTabs(tabId)}
  />

  {#if $settingsStore.markdownToolbarEnabled && $uiStore.layoutMode !== 'preview' && $activeEditor?.loadState === 'loaded'}
    <div class="markdown-toolbar" aria-label={$translator('app.markdownToolbar')}>
      {#each toolbarItems as item}
        <button
          type="button"
          title={item.shortcut
            ? `${$translator(item.labelKey)} ${item.shortcut}`
            : $translator(item.labelKey)}
          on:click={() => applyToolbar(item.action)}
        >
          <svelte:component this={item.icon} size={16} />
        </button>
      {/each}
    </div>
  {/if}

  <div
    class="workspace"
    class:no-sidebar={!effectiveSidebarVisible}
    style={`--sidebar-width: ${$uiStore.sidebarWidth.toFixed(2)}px;`}
  >
    {#if effectiveSidebarVisible}
      <Sidebar
        activeSidebarTab={$uiStore.sidebarTab}
        {recentFiles}
        tab={$activeSidebar}
        onTabChange={uiActions.setSidebarTab}
        onOpenRecent={(path) => void openPath(path)}
        onRemoveRecent={(path) => void removeRecent(path)}
        onRevealRecent={(path) => void revealRecent(path)}
        onJumpToLine={jumpToLine}
        onCollapse={uiActions.collapseSidebar}
      />
      <SidebarResizeSeparator
        width={$uiStore.sidebarWidth}
        onWidthChange={uiActions.setSidebarWidth}
        onCommit={uiActions.commitSidebarWidth}
        onCollapse={uiActions.collapseSidebar}
      />
    {/if}

    <main
      class={`editor-stage layout-${$uiStore.layoutMode}`}
      style={`--split-left: ${($uiStore.splitRatio * 100).toFixed(4)}%; --split-right: ${((1 - $uiStore.splitRatio) * 100).toFixed(4)}%;`}
      aria-busy={!initializationComplete}
    >
      {#if !initializationComplete}
        <section class="document-load-state" aria-live="polite">
          <strong>{$translator('app.restoring.title')}</strong>
          <span>{$translator('app.restoring.description')}</span>
        </section>
      {:else if $activeShell && $activeShell.loadState !== 'loaded'}
        <section class="document-load-state" aria-live="polite">
          {#if $activeShell.loadState === 'loading'}
            <strong>{$translator('app.loadingDocument.title', { title: $activeShell.title })}</strong>
            <span>{$translator('app.loadingDocument.description')}</span>
          {:else if $activeShell.loadState === 'error'}
            <strong>{$translator('app.loadFailed.title', { title: $activeShell.title })}</strong>
            <span>{$activeShell.loadError ?? $translator('app.fileUnavailable')}</span>
            <button type="button" class="primary-button" on:click={() => void loadTabContent($activeShell.id)}>{$translator('app.loadFailed.retry')}</button>
          {:else}
            <strong>{$activeShell.title}</strong>
            <span>{$translator('app.deferred.description')}</span>
            <button type="button" class="primary-button" on:click={() => void loadTabContent($activeShell.id)}>{$translator('app.deferred.load')}</button>
          {/if}
        </section>
      {:else}
        {#if $activeEditor && visiblePanes.editor}
          <section class="editor-pane">
            {#if MarkdownEditor}
              {#key $activeEditor.id}
                <svelte:component
                  this={MarkdownEditor}
                  bind:this={editorRef}
                  tabId={$activeEditor.id}
                  value={$activeEditor.content}
                  settings={$settingsStore}
                  serializedState={$activeEditor.editorState}
                  initialScrollPosition={$activeEditor.scrollPosition}
                  onChange={handleContentChange}
                  onDirty={handleContentDirty}
                  onCursorChange={handleCursorChange}
                  onScrollSync={handleEditorScroll}
                  onSessionChange={handleEditorSession}
                />
              {/key}
            {:else}
              <div class="document-load-state" aria-live="polite">{$translator('app.loadingEditor')}</div>
            {/if}
          </section>
        {/if}

        {#if $activeShell && visiblePanes.separator}
          <SplitPaneSeparator
            ratio={$uiStore.splitRatio}
            onRatioChange={uiActions.setSplitRatio}
            onCommit={uiActions.commitSplitRatio}
            onCollapse={uiActions.setLayoutMode}
          />
        {/if}

        {#if $activePreview && visiblePanes.preview}
          {#if PreviewPane}
            <svelte:component
              this={PreviewPane}
              bind:this={previewRef}
              html={$activePreview.html}
              tabId={$activePreview.id}
              contentRevision={$activePreview.contentRevision}
              renderedRevision={$activePreview.renderedRevision}
              sourceBlocks={$activePreview.sourceBlocks}
              virtualPreview={$activePreview.virtualPreview}
              diagrams={$activePreview.diagrams}
              diagramDiagnostics={$activePreview.diagramDiagnostics}
              settings={$settingsStore}
              documentPath={$activePreview.path}
              documentTitle={$activePreview.title}
              outline={$activePreview.outline}
              onOpenDocument={openPreviewDocument}
              onJumpToLine={jumpToLine}
              onSyncEditorLine={syncEditorToPreviewLine}
              onCopySource={copyPreviewSource}
            />
          {:else}
            <section class="preview-pane document-load-state" aria-live="polite">{$translator('app.loadingPreview')}</section>
          {/if}
        {/if}
      {/if}
    </main>
  </div>

  {#if $settingsStore.showStatusBar}
    <StatusBar
      tab={$activeStatus}
      layoutMode={$uiStore.layoutMode}
    />
  {/if}
</div>

{#if $uiStore.settingsOpen && SettingsDialog}
  <svelte:component
    this={SettingsDialog}
    open
    settings={$settingsStore}
    busy={$settingsOperationBusy}
    onSave={saveSettings}
    onReset={resetSettings}
    onClose={uiActions.closeSettings}
  />
{/if}

{#if $uiStore.commandPaletteOpen && CommandPalette}
  <svelte:component
    this={CommandPalette}
    open
    commands={commandItems}
    onClose={uiActions.closeCommandPalette}
  />
{/if}
{#if exportDialogOpen && ExportDialog}
  <svelte:component
    this={ExportDialog}
    open
    busy={exportBusy}
    progress={exportProgress}
    onCancel={() => void cancelImageExport()}
    cancelRequested={imageExportJob?.cancelRequested ?? false}
    documentTitle={$activeShell?.title ?? ''}
    onExport={(format, options) => void exportDocument(format, options)}
    onClose={closeExportDialog}
  />
{/if}
{#if warningDetailsOpen && lastExportWarnings && ExportWarningsDialog}
  <svelte:component this={ExportWarningsDialog}
    report={lastExportWarnings}
    canJump={$activeRender?.id === lastExportWarnings.tabId && $activeRender?.contentRevision === lastExportWarnings.contentRevision}
    onJumpToLine={(line) => { warningDetailsOpen = false; jumpToLine(line); }}
    onClose={() => (warningDetailsOpen = false)}
  />
{/if}
{#if $uiStore.aboutOpen && AboutDialog}
  <svelte:component
    this={AboutDialog}
    open
    onClose={uiActions.closeAbout}
    onExportDiagnostics={() => void exportStartupDiagnostics()}
    onClearDiagnostics={() => void clearStartupDiagnostics()}
    onOpenRepository={(url) => void openRepository(url)}
  />
{/if}

{#if $uiStore.markdownGuideOpen && MarkdownGuideDialog}
  <svelte:component
    this={MarkdownGuideDialog}
    open
    onClose={uiActions.closeMarkdownGuide}
  />
{/if}

{#if exitPrompt && ExitConfirmationDialog && !saveConflictPrompt}
  <svelte:component
    this={ExitConfirmationDialog}
    open
    documents={exitPrompt.documents}
    busy={exitPrompt.busy}
    busyLabel={exitPrompt.busyLabel}
    onSave={() => void exitProtection.saveAndExit()}
    onDiscard={() => void exitProtection.discardAndExit()}
    onCancel={() => exitProtection.cancel()}
  />
{/if}

{#if saveConflictPrompt && ExternalChangeDialog}
  <svelte:component
    this={ExternalChangeDialog}
    open
    title={saveConflictPrompt.title}
    path={saveConflictPrompt.path}
    busy={saveConflictPrompt.busy}
    onReload={() => void reloadConflictedDocument()}
    onSaveCopy={() => void saveConflictedCopy()}
    onOverwrite={() => void overwriteConflictedDocument()}
    onCancel={closeSaveConflictPrompt}
  />
{/if}

<div class="toast-stack">
  {#if lastExportWarnings}
    <div class="toast info" role="status">
      <button type="button" on:click={() => (warningDetailsOpen = true)}>
        {$translator('export.warningDetails', { count: lastExportWarnings.warnings.length })}
      </button>
      <button type="button" aria-label={$translator('export.warningDismiss')} on:click={() => (lastExportWarnings = null)}>×</button>
    </div>
  {/if}
  {#each $uiStore.toasts as toast}
    <button type="button" class={`toast ${toast.tone}`} on:click={() => uiActions.dismissToast(toast.id)}>
      {toast.message}
    </button>
  {/each}
</div>
