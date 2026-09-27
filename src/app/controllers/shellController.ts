import { startupElapsedMs, type FrontendStartupStage } from '../../lib/startupLifecycle';
import type { api as Api, AppSettings, ResourceSessionDto, SessionStateDto } from '../../lib/tauriApi';
import { isSameResource, type ResourceRef } from '../../lib/platform/resources';
import type { DocumentState } from '../stores/documentStore';
import type { createSessionCoordinator } from '../../lib/sessionCoordinator';
import { createLifecycleScope } from '../../lib/lifecycleScope';

type Dependencies = {
  api: Pick<typeof Api, 'recordFrontendStartupEvent' | 'clearSession' | 'getSession' | 'getStartupFileArg'
    | 'getPlatformCapabilities' | 'getResourceSession' | 'clearResourceSession'>;
  loadSettings: () => Promise<boolean>;
  settings: () => AppSettings;
  state: () => DocumentState;
  setSidebarVisible: (visible: boolean) => void;
  refreshRecentFiles: () => Promise<boolean>;
  restoreTabs: (paths: string[], activePath: string | null) => unknown;
  restoreResourceTabs: (resources: ResourceRef[], activeResource: ResourceRef | null) => unknown;
  onResourceSessionMode: (enabled: boolean) => void;
  sessionCoordinator: Pick<ReturnType<typeof createSessionCoordinator>, 'queue' | 'flush' | 'setWritable'>;
  onSessionReady: () => void;
  setupExternalOpenListener: () => Promise<boolean>;
  openPath: (path: string) => Promise<unknown>;
  renderActiveNow: () => Promise<unknown>;
  setupDragDrop: () => Promise<boolean>;
  flushEditor: () => void;
  destroyWindow: () => Promise<void>;
  onError: (error: unknown) => void;
  onKey: (event: KeyboardEvent) => void;
};

/** Coordinates startup stages, session persistence and browser close protection. */
export function createShellController(deps: Dependencies) {
  const { api, loadSettings, settings, state, setSidebarVisible, refreshRecentFiles,
    restoreTabs, restoreResourceTabs, onResourceSessionMode, sessionCoordinator, onSessionReady,
    setupExternalOpenListener, openPath,
    renderActiveNow, setupDragDrop, flushEditor, destroyWindow, onError } = deps;
  const initializationStartedAt = performance.now();
  const scope = createLifecycleScope();
  let allowBrowserUnload = false;
  let unloadTimer: number | undefined;
  let resourceSessionMode = false;
  async function initialize() {
    recordStartupStage('initialization', 'started');
    try {
      await runStartupStage('settings', async () => {
        const loaded = await loadSettings();
        const currentSettings = settings();
        setSidebarVisible(currentSettings.showSidebar);
        return loaded;
      });
      const currentSettings = settings();
      resourceSessionMode = (await api.getPlatformCapabilities()).platform === 'android';
      onResourceSessionMode(resourceSessionMode);
      await runStartupStage('recentFiles', refreshRecentFiles);
      const sessionWritable = await runStartupStage('sessionRestore', () => restoreSession(currentSettings.restoreLastSession));
      sessionCoordinator.setWritable(sessionWritable);
      onSessionReady();
      await runStartupStage('externalListeners', setupExternalOpenListener);
      await runStartupStage('startupFile', openStartupFile);
      await runStartupStage('firstRender', async () => { await renderActiveNow(); });
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
        if (resourceSessionMode) await api.clearResourceSession();
        else await api.clearSession();
        return true;
      } catch (error) {
        onError(error);
        return false;
      }
    }

    try {
      if (resourceSessionMode) {
        const session = await api.getResourceSession();
        restoreResourceTabs(session.resources, session.activeResource);
      } else {
        const session = await api.getSession();
        restoreTabs(session.paths.slice(0, 50), session.activePath);
      }
      return true;
    } catch (error) {
      onError(error);
      return false;
    }
  }

  function currentSessionSnapshot(state: DocumentState): SessionStateDto | ResourceSessionDto {
    if (resourceSessionMode) {
      const resources: ResourceRef[] = [];
      for (const tab of state.tabs) {
        if (tab.resource && (tab.resource.kind === 'androidDocument' || tab.resource.kind === 'desktopFile')
          && !resources.some(resource => isSameResource(resource, tab.resource))) resources.push(tab.resource);
        if (resources.length === 50) break;
      }
      const activeResource = state.tabs.find(tab => tab.id === state.activeTabId)?.resource ?? null;
      return { version: 2, resources, activeResource: resources.find(resource =>
        isSameResource(resource, activeResource)) ?? null };
    }
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

  async function flushSession(): Promise<void> {
    flushEditor();
    sessionCoordinator.queue({
      enabled: settings().restoreLastSession,
      session: currentSessionSnapshot(state())
    });
    await sessionCoordinator.flush();
  }

  async function closeApplicationWindow(): Promise<void> {
    await flushSession();
    allowBrowserUnload = true;
    try {
      await destroyWindow();
    } catch (error) {
      allowBrowserUnload = false;
      throw error;
    }

    unloadTimer = window.setTimeout(() => {
      allowBrowserUnload = false;
    }, 500);
  }

  return {
    initialize, close: closeApplicationWindow, flushSession,
    mount() {
      const beforeUnload = (event: BeforeUnloadEvent) => {
        if (!allowBrowserUnload && state().tabs.some(tab => tab.isDirty)) {
          event.preventDefault(); event.returnValue = '';
        }
      };
      window.addEventListener('keydown', deps.onKey);
      window.addEventListener('beforeunload', beforeUnload);
      scope.own(() => window.removeEventListener('keydown', deps.onKey));
      scope.own(() => window.removeEventListener('beforeunload', beforeUnload));
    },
    dispose() {
      if (unloadTimer !== undefined) window.clearTimeout(unloadTimer);
      return scope.dispose();
    }
  };
}
