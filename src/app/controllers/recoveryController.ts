import type { DocumentState, EditorTab } from '../stores/documentStore';
import { createRecoveryCoordinator } from '../../lib/recoveryCoordinator';
import type { RecoveryEntry, RecoveryInventory, RecoverySnapshot } from '../../lib/platform/recovery';
import type { TauriClient } from '../../lib/platform/tauriClient';
import type { ResourceRef } from '../../lib/platform/resources';
import type { FileVersionDto } from '../../lib/platform/contracts';

export type RecoverySourceStatus = 'untitled' | 'unchanged' | 'changed' | 'missing' | 'unavailable';

type Dependencies = {
  api: Pick<TauriClient, 'saveRecovery' | 'resolveRecovery' | 'listRecovery' | 'readRecovery'>;
  subscribe: (receive: (state: DocumentState) => void) => () => void;
  openCopy: (snapshot: RecoverySnapshot) => EditorTab;
  flushEditor: () => void;
  onInventory: (inventory: RecoveryInventory) => void;
  onError: (error: unknown) => void;
  createId?: () => string;
  version?: (resource: ResourceRef) => Promise<FileVersionDto | null>;
};

/** Owns recovery identities; closing the process preserves copies, closing a tab retires its copy. */
export function createRecoveryController(deps: Dependencies) {
  const coordinator = createRecoveryCoordinator({ save: deps.api.saveRecovery, resolve: deps.api.resolveRecovery, onError: deps.onError });
  const documents = new Map<string, { id: string; revision: number }>();
  let unsubscribe: (() => void) | undefined;
  let disposed = false;
  let suspended = false;
  let lastState: DocumentState | undefined;
  const createId = deps.createId ?? (() => crypto.randomUUID());

  function observe(state: DocumentState) {
    lastState = state;
    if (suspended) return;
    const dirty = new Set<string>();
    for (const tab of state.tabs) {
      if (!tab.isDirty || tab.loadState !== 'loaded') continue;
      dirty.add(tab.id);
      let copy = documents.get(tab.id);
      if (!copy) {
        copy = { id: createId(), revision: -1 };
        documents.set(tab.id, copy);
      }
      if (copy.revision === tab.contentRevision) continue;
      copy.revision = tab.contentRevision;
      coordinator.queue({ id: copy.id, revision: tab.contentRevision, resource: tab.resource,
        title: tab.title, content: tab.content, baseFileIdentity: tab.fileIdentity, baseContentVersion: tab.contentVersion });
    }
    for (const [tabId, copy] of documents) {
      if (dirty.has(tabId)) continue;
      documents.delete(tabId);
      coordinator.retire(copy.id);
    }
  }

  async function refresh() {
    const inventory = await deps.api.listRecovery();
    if (disposed) return;
    const liveIds = new Set([...documents.values()].map(copy => copy.id));
    deps.onInventory({ ...inventory, entries: inventory.entries.filter(entry => !liveIds.has(entry.receipt.id)) });
  }

  function resume() {
    suspended = false;
    if (lastState) observe(lastState);
  }

  return {
    async start() {
      if (disposed || unsubscribe) return;
      unsubscribe = deps.subscribe(observe);
      await refresh();
    },
    async restore(entry: RecoveryEntry) {
      const snapshot = await deps.api.readRecovery(entry.receipt.id);
      if (snapshot.id !== entry.receipt.id || snapshot.revision !== entry.receipt.revision) {
        throw { code: 'RECOVERY_REVISION_CONFLICT', message: 'The recovery copy has changed. Refresh the list before restoring it.' };
      }
      const tab = deps.openCopy(snapshot);
      // The new tab must have a durable copy before retiring the previous one.
      await coordinator.flush();
      const copy = documents.get(tab.id);
      if (!copy || coordinator.receipt(copy.id)?.revision !== tab.contentRevision) {
        throw { code: 'RECOVERY_NOT_DURABLE', message: 'The restored tab has not been persisted. The original recovery copy was retained.' };
      }
      await deps.api.resolveRecovery(entry.receipt);
      await refresh();
    },
    async inspect(entry: RecoveryEntry): Promise<RecoverySourceStatus> {
      const snapshot = await deps.api.readRecovery(entry.receipt.id);
      if (!snapshot.resource) return 'untitled';
      try {
        if (!deps.version) return 'unavailable';
        const version = await deps.version(snapshot.resource);
        if (!version) return 'missing';
        return version.fileIdentity === snapshot.baseFileIdentity && version.contentVersion === snapshot.baseContentVersion ? 'unchanged' : 'changed';
      } catch { return 'unavailable'; }
    },
    async discard(entry: RecoveryEntry) {
      await deps.api.resolveRecovery(entry.receipt);
      await refresh();
    },
    async flush() {
      deps.flushEditor();
      await coordinator.flush();
    },
    async discardOpenCopies() {
      deps.flushEditor();
      suspended = true;
      try {
        for (const copy of documents.values()) coordinator.retire(copy.id);
        await coordinator.flush();
        documents.clear();
      } catch (error) {
        documents.clear();
        resume();
        throw error;
      }
    },
    refresh, resume,
    dispose() {
      disposed = true;
      unsubscribe?.();
      unsubscribe = undefined;
      // Do not retire on teardown: the process may be closing without a discard decision.
      coordinator.dispose();
    }
  };
}
