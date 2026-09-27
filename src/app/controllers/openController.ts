import type { ResourceStorage } from '../../lib/platform/resourceStorage';
import { desktopFile, type ResourceRef } from '../../lib/platform/resources';
import { get } from 'svelte/store';
import { createAsyncSingleFlight } from '../../lib/asyncSingleFlight';
import { localizeError, t } from '../../lib/i18n';
import { toAppError } from '../../lib/tauriApi';
import type { documentStore as Store, EditorTab } from '../stores/documentStore';

type Dependencies = {
  documentStore: Pick<typeof Store, 'newDocument' | 'openDocument' | 'setActive' | 'getTab' | 'markLoadFailed' | 'markLoading' | 'hydrateDocument' | 'subscribe' | 'closeTab' | 'closeTabs'>;
  storage: Pick<ResourceStorage, 'read'>;
  flushEditor: () => void;
  forget: (id: string) => void;
  confirmAction: (message: string, title: string) => Promise<boolean>;
  pickDocumentResource: () => Promise<ResourceRef | null>;
  refreshRecentFiles: () => Promise<boolean>;
  errorMessage: (error: unknown) => string;
  toast: (message: string, kind?: 'error' | 'info' | 'success') => void;
};

/** Owns open transactions and per-tab lazy hydration; the store checks identity. */
export function createOpenController(deps: Dependencies) {
  const { documentStore, storage, flushEditor, pickDocumentResource, refreshRecentFiles, errorMessage, toast, forget, confirmAction } = deps;
  const loadSingleFlight = createAsyncSingleFlight<string, EditorTab | null>();
  function newDocument() {
    flushEditor();
    documentStore.newDocument();
    toast(t('toast.newDocument'));
  }

  async function openFile() {
    try {
      const resource = await pickDocumentResource();
      if (resource) {
        await openResource(resource);
      }
    } catch (error) {
      toast(errorMessage(error), 'error');
    }
  }

  async function openPath(path: string): Promise<EditorTab | null> {
    return openResource(desktopFile(path));
  }

  async function openResource(resource: ResourceRef): Promise<EditorTab | null> {
    flushEditor();
    try {
      const result = await storage.read(resource);
      const tab = documentStore.openDocument(result.document);
      await refreshRecentFiles();
      const auxiliaryError = result.auxiliaryError;
      if (auxiliaryError) {
        toast(t('toast.openedRecentFailed', {
          title: result.document.title,
          message: localizeError(auxiliaryError)
        }), 'error');
      } else {
        toast(t('toast.opened', { title: result.document.title }));
      }
      return tab;
    } catch (error) {
      toast(errorMessage(error), 'error');
      return null;
    }
  }

  async function activateTab(tabId: string) {
    flushEditor();
    documentStore.setActive(tabId);
    await loadTabContent(tabId);
  }

  async function loadTabContent(tabId: string): Promise<EditorTab | null> {
    const request = loadSingleFlight.run(tabId, async () => {
      const tab = documentStore.getTab(tabId);
      if (!tab) return null;
      if (tab.loadState === 'loaded') return tab;
      if (!tab.resource) {
        documentStore.markLoadFailed(tabId, t('app.noReadablePath'));
        return null;
      }

      const requestedResource = tab.resource;
      documentStore.markLoading(tabId);
      try {
        const result = await storage.read(tab.resource);
        const applied = documentStore.hydrateDocument(tabId, requestedResource, result.document);
        if (!applied) return documentStore.getTab(tabId) ?? null;
        await refreshRecentFiles();
        if (result.auxiliaryError) {
          toast(t('toast.loadedRecentFailed', {
            title: result.document.title,
            message: localizeError(result.auxiliaryError)
          }), 'error');
        }
        return documentStore.getTab(tabId) ?? null;
      } catch (error) {
        const appError = toAppError(error);
        const message = localizeError(appError);
        documentStore.markLoadFailed(tabId, message);
        toast(message, 'error');
        return null;
      }
    });
    return request.promise;
  }

  async function closeTab(tab: EditorTab) {
    flushEditor();
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
    forget(current.id);
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
    flushEditor();
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
    for (const tab of currentTabs) forget(tab.id);
  }

  return { close: closeTab, closeById: closeTabById, closeOthers: closeOtherTabs, closeRight: closeRightTabs, create: newDocument, pick: openFile, open: openPath, openResource, activate: activateTab, load: loadTabContent };
}
