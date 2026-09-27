import type { ResourceStorage } from '../../lib/platform/resourceStorage';
import { desktopFile, isSameResource, type ResourceRef } from '../../lib/platform/resources';
import { get } from 'svelte/store';
import { createAsyncSingleFlight } from '../../lib/asyncSingleFlight';
import { saveDocumentSnapshot } from '../../lib/documentSave';
import { isSameFileIdentity } from '../../lib/filePathIdentity';
import { t, localizeError } from '../../lib/i18n';
import { toAppError, type DocumentOperationDto } from '../../lib/tauriApi';
import type { DirtyExitDocument } from '../../lib/exitProtection';
import type { documentStore as Store, EditorTab } from '../stores/documentStore';

export type SaveConflict = { tabId: string; resource: ResourceRef; path: string; title: string; busy: boolean };
type Dependencies = {
  documentStore: typeof Store;
  storage: ResourceStorage;
  activeTab: () => EditorTab | null;
  flushEditor: () => void;
  loadTab: (id: string) => Promise<EditorTab | null>;
  pickSaveResource: (defaultName?: string | null) => Promise<ResourceRef | null>;
  refreshRecentFiles: () => Promise<boolean>;
  errorMessage: (error: unknown) => string;
  toast: (message: string, kind?: 'error' | 'info' | 'success') => void;
  closeModals: () => void;
  onConflict: (prompt: SaveConflict | null) => void;
  flushRecovery?: () => Promise<void>;
};

/** Owns save serialization, conflict decisions and autosave failure deduplication. */
export function createSaveController(deps: Dependencies) {
  const { documentStore, storage, activeTab, flushEditor, loadTab, pickSaveResource,
    refreshRecentFiles, errorMessage, toast, closeModals, onConflict } = deps;
  const saveSingleFlight = createAsyncSingleFlight<string, DocumentOperationDto | null>();
  const autosaveFailureKeys = new Map<string, string>();
  const autosaveConflictTabs = new Set<string>();
  let saveConflictPrompt: SaveConflict | null = null;
  function publish(prompt: SaveConflict | null) {
    saveConflictPrompt = prompt;
    onConflict(prompt);
  }
  async function saveActive(showToast = true) {
    flushEditor();
    let tab = activeTab();
    if (!tab) return;
    if (tab.loadState !== 'loaded') {
      const loadedTab = await loadTab(tab.id);
      if (!loadedTab || loadedTab.loadState !== 'loaded') return;
      tab = loadedTab;
    }

    try {
      const resource = tab.resource ?? (await pickSaveResource(tab.title));
      if (!resource) return;
      await saveResource(tab.id, resource, showToast);
    } catch (error) {
      toast(errorMessage(error), 'error');
    }
  }

  async function saveActiveAs() {
    flushEditor();
    let tab = activeTab();
    if (!tab) return;
    if (tab.loadState !== 'loaded') {
      const loadedTab = await loadTab(tab.id);
      if (!loadedTab || loadedTab.loadState !== 'loaded') return;
      tab = loadedTab;
    }

    try {
      const resource = await pickSaveResource(tab.path ?? tab.title);
      if (!resource) return;
      await saveResource(tab.id, resource, true);
    } catch (error) {
      toast(errorMessage(error), 'error');
    }
  }

  async function saveResource(
    tabId: string,
    resource: ResourceRef,
    showToast: boolean,
    overwriteExternalChanges = false
  ): Promise<boolean> {
    if (tabId === activeTab()?.id) flushEditor();
    const request = saveSingleFlight.run(tabId, async () => {
      try { await deps.flushRecovery?.(); }
      catch (error) { toast(errorMessage(error), 'error'); }
      const result = await saveDocumentSnapshot(tabId, resource, {
        getTab: documentStore.getTab,
        resolveFileVersion: storage.version,
        getFileOwner: documentStore.getFileOwner,
        activateTab: documentStore.setActive,
        saveFile: (target, content, expectedFileIdentity, expectedContentVersion, overwriteContentConflict) => storage.write({ resource: target, content, expectedFileIdentity, expectedContentVersion, overwriteContentConflict }),
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
        const requestedVersion = await storage.version(resource, true);
        const matchesRequestedTarget = requestedVersion
          ? isSameFileIdentity(current.fileIdentity, requestedVersion.fileIdentity)
          : isSameResource(current.resource, resource);
        if (current.isDirty || !matchesRequestedTarget) {
          return saveResource(tabId, resource, showToast);
        }
      }

      const savedMessage = current?.isDirty
        ? t('toast.savedSnapshot', { title: saved.title })
        : t('toast.saved', { title: saved.title });
      if (result.auxiliaryError) {
        const failureKey = `${result.auxiliaryError.code}:${result.auxiliaryError.message}`;
        if (showToast || autosaveFailureKeys.get(tabId) !== failureKey) {
          toast(t('toast.savedRecentFailed', {
            saved: savedMessage,
            message: localizeError(result.auxiliaryError)
          }), 'error');
        }
        if (current) autosaveFailureKeys.set(tabId, failureKey);
      } else {
        autosaveFailureKeys.delete(tabId);
        autosaveConflictTabs.delete(tabId);
        if (showToast) {
          toast(savedMessage, current?.isDirty ? 'info' : 'success');
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
          closeModals();
          publish({ tabId, resource, path: resource.kind === 'desktopFile' ? resource.path : current.title, title: current.title, busy: false });
        }
      }
      const failureKey = `${appError.code}:${appError.message}`;
      if (showToast || autosaveFailureKeys.get(tabId) !== failureKey) {
        toast(localizeError(appError), 'error');
      }
      if (documentStore.getTab(tabId)) autosaveFailureKeys.set(tabId, failureKey);
      return false;
    }
  }

  function closeSaveConflictPrompt(): void {
    if (!saveConflictPrompt?.busy) publish(null);
  }

  async function reloadConflictedDocument(): Promise<void> {
    flushEditor();
    const prompt = saveConflictPrompt;
    if (!prompt || prompt.busy) return;
    publish({ ...prompt, busy: true });
    try {
      const result = await storage.read(prompt.resource);
      if (!documentStore.reloadDocument(prompt.tabId, prompt.resource, result.document)) {
        throw Object.assign(new Error('The conflicted tab changed before reload completed.'), {
          code: 'FILE_IDENTITY_CONFLICT'
        });
      }
      autosaveConflictTabs.delete(prompt.tabId);
      autosaveFailureKeys.delete(prompt.tabId);
      publish(null);
      if (result.auxiliaryError) toast(localizeError(result.auxiliaryError), 'error');
    } catch (error) {
      publish({ ...prompt, busy: false });
      toast(errorMessage(error), 'error');
    }
  }

  async function saveConflictedCopy(): Promise<void> {
    const prompt = saveConflictPrompt;
    if (!prompt || prompt.busy) return;
    try {
      const resource = await pickSaveResource(prompt.title);
      if (!resource) return;
      publish({ ...prompt, busy: true });
      const saved = await saveResource(prompt.tabId, resource, true);
      if (saved) {
        autosaveConflictTabs.delete(prompt.tabId);
        publish(null);
      } else if (saveConflictPrompt?.tabId === prompt.tabId && saveConflictPrompt.busy) {
        publish({ ...prompt, busy: false });
      }
    } catch (error) {
      publish({ ...prompt, busy: false });
      toast(errorMessage(error), 'error');
    }
  }

  async function overwriteConflictedDocument(): Promise<void> {
    const prompt = saveConflictPrompt;
    if (!prompt || prompt.busy) return;
    publish({ ...prompt, busy: true });
    const saved = await saveResource(prompt.tabId, prompt.resource, true, true);
    if (saved) {
      autosaveConflictTabs.delete(prompt.tabId);
      publish(null);
    } else if (saveConflictPrompt?.tabId === prompt.tabId && saveConflictPrompt.busy) {
      publish({ ...prompt, busy: false });
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
      toast(t('toast.unloadedCannotSave', { title: tab.title }), 'error');
      return false;
    }

    try {
      const resource = tab.resource ?? (await pickSaveResource(tab.title));
      if (!resource) {
        toast(t('toast.saveCancelled', { title: tab.title }), 'info');
        return false;
      }

      const saved = await saveResource(tab.id, resource, true);
      if (!saved) {
        const current = documentStore.getTab(tab.id);
        if (current?.isDirty) {
          toast(t('toast.stillDirty', { title: current.title }), 'info');
        }
      }
      return saved;
    } catch (error) {
      toast(t('toast.saveFailed', { title: tab.title, message: errorMessage(error) }), 'error');
      return false;
    }
  }

  return {
    save: (tabId: string, path: string, showToast: boolean, overwriteExternalChanges = false) =>
      saveResource(tabId, desktopFile(path), showToast, overwriteExternalChanges),
    saveResource, saveActive, saveAs: saveActiveAs,
    closeConflict: closeSaveConflictPrompt, reload: reloadConflictedDocument,
    saveCopy: saveConflictedCopy, overwrite: overwriteConflictedDocument,
    dirtyDocuments: getDirtyExitDocuments, saveBeforeExit: saveDirtyDocumentBeforeExit,
    isBlocked: (id: string) => autosaveConflictTabs.has(id),
    forget(id: string) { autosaveFailureKeys.delete(id); autosaveConflictTabs.delete(id); }
  };
}
