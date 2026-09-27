<script lang="ts">
  import { onDestroy, onMount, tick } from 'svelte';
  import WorkspacePanel from '../features/workspace/WorkspacePanel.svelte';
  import { createWorkspaceStore } from '../features/workspace/store';
  import { pickWorkspaceDirectory } from '../lib/platform/dialogs';
  import { listenWorkspaceChanges } from '../lib/platform/workspaceEvents';
  import { desktopFile, type ResourceRef } from '../lib/platform/resources';
  import { createAndroidDocuments } from '../lib/platform/androidDocuments';
  import { androidAvailableViewportHeight, exitAndroidApplication, getAndroidSystemInsets, parseAndroidSystemInsets, setAndroidStatusBarAppearance, type AndroidSystemInsets } from '../lib/platform/androidLifecycle';
  import { get } from 'svelte/store';
  import Sidebar from '../components/layout/Sidebar.svelte';
  import DocumentTabBar from '../components/layout/DocumentTabBar.svelte';
  import SplitPaneSeparator from '../components/layout/SplitPaneSeparator.svelte';
  import SidebarResizeSeparator from '../components/layout/SidebarResizeSeparator.svelte';
  import StatusBar from '../components/layout/StatusBar.svelte';
  import UpdateBanner from '../features/updates/UpdateBanner.svelte';
  import { createUpdateService, type UpdateView } from '../features/updates/service';
  import { prepareInstall } from '../features/updates/installBarrier';
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
    resourceSessionProjection,
    tabBarState,
    type CursorPosition,
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
    type ExportFormat,
    type ExportOptions,
    type RecentFileDto,
    type SessionStateDto,
    type ResourceSessionDto
  } from '../lib/tauriApi';
  import { toolbarItems, type ToolbarAction } from '../lib/markdownToolbar';
  import {
    buildCommandItems,
    findKeyboardCommand,
    type CommandActions,
    type CommandId,
    type CommandItem
  } from '../lib/commands';
  import { createOpenController } from './controllers/openController';
  import { createNativeController } from './controllers/nativeController';
  import { createSaveController, type SaveConflict } from './controllers/saveController';
  import {
    createExitProtectionController,
    type DirtyExitDocument,
    type ExitPromptState
  } from '../lib/exitProtection';
  import RecoveryPanel from '../components/dialogs/RecoveryPanel.svelte';
  import { createRecoveryController } from './controllers/recoveryController';
  import { createResidencyController } from './controllers/residencyController';
  import type { RecoveryInventory } from '../lib/platform/recovery';
  import { createShellController } from './controllers/shellController';
  import { type ExportProgressView } from '../lib/exportProgress';
  import { createExportController } from './controllers/exportController';
  import { createResourceStorage } from '../lib/platform/resourceStorage';
  import { desktopEvents, listenExportProgress } from '../lib/platform/desktopEvents';
  import { type ExportWarningReport } from '../lib/exportWarningReport';
  import { appInitialization } from '../lib/appInitialization';
  import { createLifecycleScope } from '../lib/lifecycleScope';
  import { shouldHandleGlobalShortcut } from '../lib/shortcutGuard';
  import { observeMediaQuery } from '../lib/mediaQuery';
  import { createMarkdownRenderCoordinator, type MarkdownRenderRequest } from '../lib/markdownRenderCoordinator';
  import { paneVisibility } from '../lib/layoutVisibility';
  import { createAutosaveScheduler } from '../lib/autosaveScheduler';
  import { createLazyComponent } from '../lib/lazyComponent';
  import { createSessionCoordinator } from '../lib/sessionCoordinator';
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
    refreshDiagrams: () => void;
  };

  type MarkdownEditorProps = {
    tabId: string;
    value: string;
    settings: AppSettings;
    mobile: boolean;
    serializedState: import('../lib/editorSession').EditorSnapshot | null;
    initialScrollPosition: EditorScrollPosition;
    onChange: (tabId: string, value: string, lineCount: number) => void;
    onDirty: (tabId: string) => void;
    onCursorChange: (tabId: string, position: CursorPosition) => void;
    onScrollSync: (tabId: string, position: EditorScrollPosition, userInitiated: boolean) => void;
    onSessionChange: (tabId: string, state: import('../lib/editorSession').EditorSnapshot) => void;
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
    markdownDiagnostics: import('../lib/tauriApi').MarkdownDiagnostic[];
    settings: AppSettings;
    mobile: boolean;
    documentResource: ResourceRef | null;
    documentTitle: string;
    outline: import('../lib/tauriApi').OutlineItem[];
    onOpenDocument: (resource: ResourceRef, fragment: string | null) => Promise<boolean>;
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
  let androidRecentFiles: import('../lib/platform/androidDocuments').AndroidRecentDocument[] = [];
  let autosaveTimer: number | undefined;
  let autosaveKey = '';
  let lastSessionKey = '';
  let sessionInitialized = false;
  let resourceSessionMode = false;
  let pendingPreviewFragment: { tabId: string; fragment: string } | null = null;
  let previewFragmentTimer: number | undefined;
  const autosaveScheduler = createAutosaveScheduler(
    () => get(documentStore).tabs.map((tab) => ({
      id: tab.id,
      resource: tab.resource,
      dirty: tab.isDirty,
      loaded: tab.loadState === 'loaded',
      blocked: saveController.isBlocked(tab.id)
    })),
    (tabId, resource) => saveController.saveResource(tabId, resource, false)
  );
  const lifecycleScope = createLifecycleScope();
  const residencyController = createResidencyController({
    subscribe: documentStore.subscribe,
    evict: documentStore.evictClean,
    releasePreview: api.releaseMarkdownPreview,
    onError: error => uiActions.toast(errorMessage(error), 'error')
  });
  const sessionCoordinator = createSessionCoordinator({
    persist: async ({ enabled, session }) => {
      if (enabled) {
        if (session.version === 2) await api.updateResourceSession(session);
        else await api.updateSession(session);
      } else if (session.version === 2) await api.clearResourceSession();
      else await api.clearSession();
    },
    onError: (error) => uiActions.toast(errorMessage(error), 'error')
  });
  let recoveryInventory: RecoveryInventory = { entries: [], issues: [] };
  let recoveryPanelOpen = false;
  const recoveryController = createRecoveryController({
    api, subscribe: documentStore.subscribe, flushEditor: flushActiveEditor,
    version: resource => resourceStorage.version(resource, true),
    openCopy: snapshot => documentStore.openDocument({
      path: null, resource: null, fileIdentity: null, contentVersion: null,
      title: snapshot.title, content: snapshot.content, isDirty: true, lastSavedAt: null, fileSize: null
    }),
    onInventory: inventory => { recoveryInventory = inventory; },
    onError: error => uiActions.toast(errorMessage(error), 'error')
  });
  let exitPrompt: ExitPromptState | null = null;
  let saveConflictPrompt: SaveConflict | null = null;
  let exportDialogOpen = false;
  let exportBusy = false;
  let exportProgress: ExportProgressView | null = null;
  let exportCancelRequested = false;
  let lastExportWarnings: ExportWarningReport | null = null;
  let warningDetailsOpen = false;
  let initializationComplete = false;
  let desktopUpdates = false;
  let desktopDiagramPack = false;
  let updateView: UpdateView = { phase: 'idle' };
  let updateService: ReturnType<typeof createUpdateService> | null = null;
  let unsubscribeUpdates: (() => void) | null = null;
  const narrowViewportQuery = window.matchMedia?.('(max-width: 920px)');
  let narrowViewport = narrowViewportQuery?.matches ?? false;
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
  $: effectiveSidebarVisible = showAndroidFolderAction && narrowViewport ? mobileDrawerOpen : $uiStore.sidebarVisible;
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
    const session: SessionStateDto | ResourceSessionDto = resourceSessionMode
      ? { version: 2, ...$resourceSessionProjection }
      : { version: 1, ...$sessionProjection };
    const sessionKey = `${$settingsStore.restoreLastSession}:${JSON.stringify(session)}`;
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

  let showAndroidFolderAction = false;
  let availableExportFormats: ExportFormat[] = ['html', 'pdf', 'docx', 'png', 'svg'];
  let allowLocalImageExport = true;
  let androidExport = false;
  let mobileDrawerOpen = false;
  let mobileMenuOpen = false;
  let compactAndroidIme = false;
  let shellElement: HTMLDivElement;
  function observeAndroidViewport(): () => void {
    const viewport = window.visualViewport;
    if (!viewport) return () => {};
    let imeBottom = 0;
    const updateHeight = () => {
      const availableHeight = androidAvailableViewportHeight(window.innerHeight, viewport.height, imeBottom);
      shellElement.style.setProperty('--android-viewport-height', `${availableHeight}px`);
      // Below this height the title, tabs, toolbar and status leave less than
      // three editor lines. Keep the save control and content while the IME is open.
      compactAndroidIme = imeBottom > 0 && availableHeight < 260;
    };
    const applyInsets = (insets: AndroidSystemInsets) => {
      imeBottom = insets.imeBottom;
      shellElement.style.setProperty('--android-system-top', `${insets.top}px`);
      shellElement.style.setProperty('--android-system-right', `${insets.right}px`);
      shellElement.style.setProperty('--android-system-bottom', `${imeBottom > 0 ? 0 : insets.bottom}px`);
      shellElement.style.setProperty('--android-system-left', `${insets.left}px`);
      updateHeight();
    };
    const updateInsets = (event: Event) => {
      try { applyInsets(parseAndroidSystemInsets((event as CustomEvent<unknown>).detail)); }
      catch { /* Ignore malformed native events; the initial command still supplies insets. */ }
    };
    updateHeight();
    viewport.addEventListener('resize', updateHeight);
    window.addEventListener('marklite:system-insets', updateInsets);
    void getAndroidSystemInsets().then(applyInsets)
      .catch(error => uiActions.toast(errorMessage(error), 'error'));
    return () => {
      viewport.removeEventListener('resize', updateHeight);
      window.removeEventListener('marklite:system-insets', updateInsets);
      compactAndroidIme = false;
      shellElement.style.removeProperty('--android-viewport-height');
      for (const side of ['top', 'right', 'bottom', 'left']) {
        shellElement.style.removeProperty(`--android-system-${side}`);
      }
    };
  }
  function toggleSidebar(): void {
    if (showAndroidFolderAction && narrowViewport) mobileDrawerOpen = !mobileDrawerOpen;
    else uiActions.toggleSidebar();
  }
  function collapseSidebar(): void {
    if (showAndroidFolderAction && narrowViewport) mobileDrawerOpen = false;
    else uiActions.collapseSidebar();
  }
  function handleAndroidBack(): void {
    if (mobileMenuOpen) { mobileMenuOpen = false; return; }
    if (mobileDrawerOpen) { mobileDrawerOpen = false; return; }
    if (saveConflictPrompt) {
      if (!saveConflictPrompt.busy) closeSaveConflictPrompt();
      return;
    }
    if (exitPrompt) { exitProtection.cancel(); return; }
    if (warningDetailsOpen) { warningDetailsOpen = false; return; }
    if (exportDialogOpen) { closeExportDialog(); return; }
    if ($uiStore.settingsOpen || $uiStore.commandPaletteOpen || $uiStore.aboutOpen || $uiStore.markdownGuideOpen) {
      uiActions.closeModals();
      return;
    }
    flushActiveEditor();
    exitProtection.handleCloseRequest(() => {});
  }
  onMount(() => {
    if (isTauriRuntime()) {
      void api.getPlatformCapabilities().then(async capabilities => {
        availableExportFormats = capabilities.exportFormats;
        allowLocalImageExport = capabilities.desktopFiles || capabilities.documentUris;
        androidExport = capabilities.platform === 'android';
        desktopDiagramPack = capabilities.desktopFiles;
        if (capabilities.platform !== 'android') return;
        showAndroidFolderAction = true;
        applyTheme(get(settingsStore));
        lifecycleScope.own(observeAndroidViewport());
        window.addEventListener('marklite:android-back', handleAndroidBack);
        lifecycleScope.own(() => window.removeEventListener('marklite:android-back', handleAndroidBack));
        lifecycleScope.own(await androidDocuments.listenOpenRequests(() => {
          if (initializationComplete) void drainAndroidOpenRequests();
        }));
        if (initializationComplete) void drainAndroidOpenRequests();
      }).catch(error => uiActions.toast(errorMessage(error), 'error'));
    }
    residencyController.start();
    void shellController.initialize()
      .then(async () => {
        initializationComplete = true;
        void workspaceStore.start();
        if (isTauriRuntime()) void recoveryController.start().catch(error => uiActions.toast(errorMessage(error), 'error'));
        await tick();
        appInitialization.succeed();
        if (isTauriRuntime()) {
          void startDesktopUpdates();
          void drainAndroidOpenRequests();
        }
      })
      .catch((error) => {
        appInitialization.fail();
        uiActions.toast(t('toast.initializationIncomplete', { message: errorMessage(error) }), 'error');
      });
    if (isTauriRuntime()) {
      void setupCloseProtection();
    }

    shellController.mount();
    const checkpointRecovery = () => {
      if (isTauriRuntime() && document.visibilityState === 'hidden') {
        void recoveryController.flush().catch(error => uiActions.toast(errorMessage(error), 'error'));
      }
    };
    const onVisibilityChange = () => {
      checkpointRecovery();
      if (document.visibilityState === 'visible') void drainAndroidOpenRequests();
    };
    document.addEventListener('visibilitychange', onVisibilityChange);
    lifecycleScope.own(() => document.removeEventListener('visibilitychange', onVisibilityChange));
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
    workspaceStore.dispose();
    residencyController.dispose();
    recoveryController.dispose();
    exportController.dispose();
    nativeController.dispose();
    shellController.dispose();
    markdownCoordinator.dispose();
    autosaveScheduler.dispose();
    if (previewFragmentTimer !== undefined) window.clearTimeout(previewFragmentTimer);
    if (autosaveTimer) window.clearInterval(autosaveTimer);
    sessionCoordinator.dispose();
    unsubscribeUpdates?.();
    void updateService?.dispose();
  });

  async function startDesktopUpdates() {
    try {
      const capabilities = await api.getPlatformCapabilities();
      if (!capabilities.desktopFiles || !['windows', 'linux', 'macos'].includes(capabilities.platform)) return;
      const { desktopUpdateAdapter } = await import('../features/updates/desktop');
      updateService = createUpdateService({
        adapter: desktopUpdateAdapter,
        preferences: {
          getItem: key => window.localStorage.getItem(key),
          setItem: (key, value) => window.localStorage.setItem(key, value)
        },
        autoEnabled: () => get(settingsStore).checkUpdatesAutomatically,
        prepareInstall: prepareUpdateInstall,
        reportError: errorMessage
      });
      unsubscribeUpdates = updateService.subscribe(view => { updateView = view; });
      desktopUpdates = true;
      void updateService.check();
    } catch (error) {
      console.warn('Desktop updates unavailable', error);
    }
  }

  async function checkUpdatesManually() {
    uiActions.closeAbout();
    if (!updateService) return;
    await updateService.check(true);
    if (updateView.phase === 'idle') uiActions.toast(t('update.noUpdate'));
  }

  async function prepareUpdateInstall(): Promise<boolean> {
    const result = await prepareInstall({
      flushEditor: flushActiveEditor,
      isBusy: () => exportBusy || exitPrompt !== null || saveConflictPrompt !== null,
      dirtyDocuments: getDirtyExitDocuments,
      confirmSave: count => confirmAction(t('update.confirmSave', { count })),
      saveDocument: saveDirtyDocumentBeforeExit,
      flushPersistence: async () => {
        await recoveryController.flush();
        await workspaceStore.flush();
        await shellController.flushSession();
      }
    });
    if (result === 'busy') uiActions.toast(t('update.exportBusy'), 'info');
    if (result === 'save-failed') uiActions.toast(t('update.saveFailed'), 'error');
    return result === 'ready';
  }

  const nativeController = createNativeController({
    enabled: isTauriRuntime(), events: desktopEvents,
    drain: api.drainOpenFileRequests, open: openPath,
    close: (preventDefault) => { flushActiveEditor(); exitProtection.handleCloseRequest(preventDefault); },
    onError: (error) => uiActions.toast(errorMessage(error), 'error'),
    onCloseUnavailable: () => uiActions.toast(t('toast.nativeExitUnavailable'), 'error')
  });
  const setupDragDrop = nativeController.dragDrop;
  const setupExternalOpenListener = nativeController.externalOpen;
  const setupCloseProtection = nativeController.closeProtection;

  const shellController = createShellController({
    api, loadSettings: settingsStore.load, settings: () => get(settingsStore),
    state: () => get(documentStore), setSidebarVisible: uiActions.setSidebarVisible,
    refreshRecentFiles, restoreTabs: documentStore.restoreSessionTabs,
    restoreResourceTabs: documentStore.restoreResourceSessionTabs,
    onResourceSessionMode: (enabled) => { resourceSessionMode = enabled; }, sessionCoordinator,
    onSessionReady: () => {
      sessionInitialized = true;
      if (resourceSessionMode) void refreshDeferredAndroidTitles();
    },
    setupExternalOpenListener, openPath, renderActiveNow, setupDragDrop,
    flushEditor: flushActiveEditor, destroyWindow: async () => {
      await workspaceStore.flush();
      if (resourceSessionMode) await exitAndroidApplication();
      else await desktopEvents.destroy();
    },
    onError: (error) => uiActions.toast(errorMessage(error), 'error'), onKey: handleGlobalShortcut
  });

  function applyTheme(settings: AppSettings) {
    if (typeof document === 'undefined') return;

    const prefersDark = window.matchMedia?.('(prefers-color-scheme: dark)').matches ?? false;
    const theme = settings.theme === 'system' ? (prefersDark ? 'dark' : 'light') : settings.theme;
    const root = document.documentElement;
    root.dataset.theme = theme;
    root.style.setProperty('--accent-color', settings.accentColor);
    root.style.setProperty('--radius-md', `${settings.cornerRadius}px`);
    if (showAndroidFolderAction) {
      void setAndroidStatusBarAppearance(theme === 'dark').catch(error => uiActions.toast(errorMessage(error), 'error'));
    }
  }

  async function renderActiveNow() {
    let tab: EditorTab | null = getActiveTab();
    // Session restoration publishes file identity first. Reading its content here
    // would defeat deferred hydration and mount the editor during startup.
    if (tab?.loadState === 'unloaded') return;
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
      if ((await api.getPlatformCapabilities()).platform === 'android') {
        androidRecentFiles = await androidDocuments.getRecentDocuments();
      } else {
        recentFiles = await api.getRecentFiles();
      }
      return true;
    } catch (error) {
      uiActions.toast(errorMessage(error), 'error');
      return false;
    }
  }

  const androidDocuments = createAndroidDocuments();
  async function refreshDeferredAndroidTitles(): Promise<void> {
    const state = get(documentStore);
    const deferred = state.tabs.filter(tab => tab.resource?.kind === 'androidDocument' &&
      tab.loadState !== 'loaded');
    const ordered = [...deferred.filter(tab => tab.id === state.activeTabId),
      ...deferred.filter(tab => tab.id !== state.activeTabId)];
    for (const tab of ordered) {
      if (tab.resource?.kind !== 'androidDocument') continue;
      try {
        const title = await androidDocuments.documentName(tab.resource);
        if (title) documentStore.setDeferredResourceTitle(tab.id, tab.resource, title);
      } catch {
        // A revoked or unavailable provider does not prevent session restoration.
      }
    }
  }
  const resourceStorage = createResourceStorage(api, androidDocuments);
  async function pickDocumentResource(): Promise<ResourceRef | null> {
    const capabilities = await api.getPlatformCapabilities();
    if (capabilities.platform === 'android') return androidDocuments.pickDocument();
    const path = await pickMarkdownFile();
    return path ? desktopFile(path) : null;
  }
  async function pickSaveResource(defaultName?: string | null): Promise<ResourceRef | null> {
    const capabilities = await api.getPlatformCapabilities();
    if (capabilities.platform === 'android') return androidDocuments.createDocument(defaultName ?? 'Untitled.md');
    const path = await pickMarkdownSavePath(defaultName);
    return path ? desktopFile(path) : null;
  }
  async function pickWorkspaceResource(): Promise<ResourceRef | null> {
    const capabilities = await api.getPlatformCapabilities();
    if (capabilities.platform === 'android') return androidDocuments.pickTree();
    const path = await pickWorkspaceDirectory();
    return path ? { kind: 'desktopDirectory', path } : null;
  }
  let drainingAndroidRequests = false;
  let drainAndroidAgain = false;
  async function drainAndroidOpenRequests(): Promise<void> {
    if (drainingAndroidRequests) {
      drainAndroidAgain = true;
      return;
    }
    drainingAndroidRequests = true;
    try {
      if ((await api.getPlatformCapabilities()).platform !== 'android') return;
      do {
        drainAndroidAgain = false;
        for (const resource of await androidDocuments.drainOpenRequests()) {
          try {
            await openController.openResource(resource);
          } catch (error) {
            uiActions.toast(errorMessage(error), 'error');
          }
        }
      } while (drainAndroidAgain);
    } catch (error) {
      uiActions.toast(errorMessage(error), 'error');
    } finally {
      drainingAndroidRequests = false;
    }
  }
  const workspaceStore = createWorkspaceStore({
    client: api.workspace, capabilities: api.getPlatformCapabilities,
    pickDirectory: pickWorkspaceResource, listen: listenWorkspaceChanges,
    open: resource => openController.openResource(resource)
  });
  const openController = createOpenController({
    documentStore, storage: resourceStorage, flushEditor: flushActiveEditor, pickDocumentResource,
    refreshRecentFiles, errorMessage, toast: uiActions.toast, confirmAction,
    forget: (id) => saveController.forget(id)
  });
  const newDocument = openController.create;
  const openFile = openController.pick;
  function openPath(path: string) { return openController.open(path); }
  const activateTab = openController.activate;
  function loadTabContent(tabId: string) { return openController.load(tabId); }

  async function openPreviewDocument(resource: ResourceRef, fragment: string | null): Promise<boolean> {
    const tab = await openController.openResource(resource);
    if (!tab) return false;
    if (!fragment) return true;
    pendingPreviewFragment = { tabId: tab.id, fragment };
    if (tab.renderedRevision === tab.contentRevision) {
      pendingPreviewFragment = null;
      window.setTimeout(() => previewRef?.scrollToFragment(fragment), 0);
    }
    return true;
  }

  const saveController = createSaveController({
    documentStore, storage: resourceStorage, activeTab: getActiveTab, flushEditor: flushActiveEditor,
    flushRecovery: async () => { if (isTauriRuntime()) await recoveryController.flush(); },
    loadTab: loadTabContent, pickSaveResource, refreshRecentFiles,
    errorMessage, toast: uiActions.toast,
    closeModals: () => { uiActions.closeModals(); exportDialogOpen = false; },
    onConflict: (prompt) => { saveConflictPrompt = prompt; }
  });
  const saveActive = saveController.saveActive;
  const saveActiveAs = saveController.saveAs;
  const saveTab = saveController.save;
  const closeSaveConflictPrompt = saveController.closeConflict;
  const reloadConflictedDocument = saveController.reload;
  const saveConflictedCopy = saveController.saveCopy;
  const overwriteConflictedDocument = saveController.overwrite;
  function getDirtyExitDocuments() { return saveController.dirtyDocuments(); }
  function saveDirtyDocumentBeforeExit(document: DirtyExitDocument) { return saveController.saveBeforeExit(document); }

  async function closeApplicationWindow(discard = false) {
    try {
      if (isTauriRuntime()) {
        if (discard) await recoveryController.discardOpenCopies();
        else await recoveryController.flush();
      }
      await shellController.close();
    } catch (error) {
      recoveryController.resume();
      throw error;
    }
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
  async function onDiagramRuntimeChanged() {
    const { previewDiagramRuntime } = await import('../lib/previewDiagramRuntime');
    previewDiagramRuntime.reset();
    previewRef?.refreshDiagrams();
  }
  const openCommandPaletteRequest = () => openStoreModal(uiActions.openCommandPalette);
  const openAboutDialogRequest = () => openStoreModal(uiActions.openAbout);
  const openMarkdownGuideRequest = () => openStoreModal(uiActions.openMarkdownGuide);

  function closeExportDialog(): void {
    if (!exportBusy) exportDialogOpen = false;
  }

  const exportController = createExportController({
    api, flushEditor: flushActiveEditor, activeTab: getActiveTab, loadTab: loadTabContent,
    pickExportSavePath, pickExportParentDirectory,
    subscribe: isTauriRuntime() ? listenExportProgress : undefined,
    errorMessage, toast: uiActions.toast,
    setBusy: (busy) => { exportBusy = busy; },
    setProgress: (view) => { exportProgress = view; },
    setCancelRequested: (requested) => { exportCancelRequested = requested; },
    closeDialog: () => { exportDialogOpen = false; },
    setWarnings: (warnings) => { lastExportWarnings = warnings; }
  });
  const exportDocument = exportController.run;
  const cancelActiveExport = exportController.cancel;

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

  const closeTab = openController.close;
  const closeTabById = openController.closeById;
  const closeOtherTabs = openController.closeOthers;
  const closeRightTabs = openController.closeRight;

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

  function handleEditorSession(tabId: string, state: import('../lib/editorSession').EditorSnapshot) {
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

  async function removeAndroidRecent(resource: ResourceRef) {
    try {
      androidRecentFiles = await androidDocuments.removeRecentDocument(resource);
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
        if (resourceSessionMode) await api.clearResourceSession();
        else await api.clearSession();
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
      sidebar: toggleSidebar,
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
  bind:this={shellElement}
  class="app-shell"
  class:android-mobile={showAndroidFolderAction}
  class:android-ime-compact={compactAndroidIme}
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
    onOpenFolder={() => void workspaceStore.choose()}
    showFolderAction={showAndroidFolderAction}
    mobile={showAndroidFolderAction && narrowViewport}
    recoveryAvailable={recoveryInventory.entries.length > 0 || recoveryInventory.issues.length > 0}
    recoveryCount={recoveryInventory.entries.length}
    onOpenRecovery={() => (recoveryPanelOpen = true)}
    bind:moreOpen={mobileMenuOpen}
    onSave={() => void saveActive()}
    onSaveAs={() => void saveActiveAs()}
    onExport={openExportDialog}
    onFind={() => editorRef?.openFind()}
    onSettings={openSettingsDialogRequest}
    onToggleSidebar={toggleSidebar}
    onLayout={setLayoutMode}
    onCommandPalette={openCommandPaletteRequest}
    onMarkdownGuide={openMarkdownGuideRequest}
    onAbout={openAboutDialogRequest}
  />
  {#if desktopUpdates}
    <UpdateBanner
      state={updateView}
      onDownload={() => void updateService?.download()}
      onInstall={() => void updateService?.install()}
      onDismiss={() => void updateService?.dismiss()}
      onSkip={() => void updateService?.dismiss(true)}
      onOpenReleases={() => void openRepository('https://github.com/Vazone/Marklite/releases/latest')}
    />
  {/if}

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
    {#if showAndroidFolderAction && narrowViewport && mobileDrawerOpen}
      <button class="mobile-sidebar-backdrop" type="button" aria-label={$translator('titlebar.sidebar.collapse')} on:click={collapseSidebar}></button>
    {/if}
    {#if effectiveSidebarVisible}
      <Sidebar
        activeSidebarTab={$uiStore.sidebarTab}
        {recentFiles}
        {androidRecentFiles}
        activeResource={$activePreview?.resource ?? null}
        tab={$activeSidebar}
        onTabChange={uiActions.setSidebarTab}
        onOpenRecent={(path) => void openPath(path)}
        onRemoveRecent={(path) => void removeRecent(path)}
        onRevealRecent={(path) => void revealRecent(path)}
        onOpenAndroidRecent={(resource) => void openController.openResource(resource)}
        onRemoveAndroidRecent={(resource) => void removeAndroidRecent(resource)}
        onJumpToLine={jumpToLine}
        onCollapse={collapseSidebar}
      >
        <WorkspacePanel slot="files" store={workspaceStore} activePath={$activeSidebar?.path ?? null}
          onRevealPath={(path) => void revealRecent(path)} />
      </Sidebar>
      <SidebarResizeSeparator
        width={$uiStore.sidebarWidth}
        onWidthChange={uiActions.setSidebarWidth}
        onCommit={uiActions.commitSidebarWidth}
        onCollapse={collapseSidebar}
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
                  mobile={showAndroidFolderAction}
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
              markdownDiagnostics={$activePreview.analysisRevision === $activePreview.contentRevision ? $activePreview.markdownDiagnostics ?? [] : []}
              settings={$settingsStore}
              mobile={showAndroidFolderAction}
              documentResource={$activePreview.resource}
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
      mobile={showAndroidFolderAction && narrowViewport}
    />
  {/if}
</div>

{#if $uiStore.settingsOpen && SettingsDialog}
  <svelte:component
    this={SettingsDialog}
    open
    settings={$settingsStore}
    busy={$settingsOperationBusy}
    {desktopUpdates}
    {desktopDiagramPack}
    {onDiagramRuntimeChanged}
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
    onCancel={() => void cancelActiveExport()}
    cancelRequested={exportCancelRequested}
    documentTitle={$activeShell?.title ?? ''}
    {availableExportFormats}
    {allowLocalImageExport}
    {androidExport}
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
    {desktopUpdates}
    onCheckUpdates={() => void checkUpdatesManually()}
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

<RecoveryPanel inventory={recoveryInventory} bind:open={recoveryPanelOpen}
  showNotice={!showAndroidFolderAction || !narrowViewport} onRestore={recoveryController.restore}
  onDiscard={recoveryController.discard} onInspect={recoveryController.inspect}
  onError={error => uiActions.toast(errorMessage(error), 'error')} />
