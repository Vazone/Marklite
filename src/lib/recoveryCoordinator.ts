import type { RecoveryReceipt, RecoverySnapshot } from './platform/recovery';

type Options = {
  save: (snapshot: RecoverySnapshot) => Promise<RecoveryReceipt>;
  resolve: (receipt: RecoveryReceipt) => Promise<void>;
  onError: (error: unknown) => void;
  delayMs?: number;
};

/** Serializes persistence and retirement; an acknowledged revision is not an editor save. */
export function createRecoveryCoordinator(options: Options) {
  const pending = new Map<string, RecoverySnapshot>();
  const retiring = new Set<string>();
  const receipts = new Map<string, RecoveryReceipt>();
  let active: Promise<void> | undefined;
  let writing: RecoverySnapshot | undefined;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let disposed = false;

  function clearTimer() { if (timer !== undefined) clearTimeout(timer); timer = undefined; }
  function drain(): Promise<void> {
    if (active) return active;
    let completed = false;
    active = (async () => {
      // Freeze this batch. New edits remain pending for the next batch.
      const batch = [...pending.values()];
      const retirements = [...retiring];
      for (const next of batch) {
        if (disposed) return;
        if (retiring.has(next.id)) continue;
        if (pending.get(next.id) === next) pending.delete(next.id);
        writing = next;
        try {
          const receipt = await options.save(next);
          if (receipt.id !== next.id || receipt.revision !== next.revision) {
            throw { code: 'INVALID_RECOVERY_RECEIPT', message: 'Recovery acknowledgement does not match the submitted revision.' };
          }
          receipts.set(next.id, receipt);
        } catch (error) {
          if (!pending.has(next.id)) pending.set(next.id, next);
          throw error;
        } finally { writing = undefined; }
      }
      for (const id of retirements) {
        if (disposed) return;
        const receipt = receipts.get(id);
        if (receipt) await options.resolve(receipt);
        retiring.delete(id);
        receipts.delete(id);
      }
      completed = true;
    })().finally(() => {
      active = undefined;
      if (completed && (pending.size || retiring.size)) schedule();
    });
    return active;
  }
  function schedule() {
    // A fixed window from the first edit also persists during continuous typing.
    if (timer !== undefined || disposed) return;
    timer = setTimeout(() => { timer = undefined; void drain().catch(options.onError); }, options.delayMs ?? 1500);
  }
  return {
    queue(snapshot: RecoverySnapshot) {
      if (disposed || retiring.has(snapshot.id)) return;
      const latest = Math.max(pending.get(snapshot.id)?.revision ?? -1,
        receipts.get(snapshot.id)?.revision ?? -1, writing?.id === snapshot.id ? writing.revision : -1);
      if (snapshot.revision <= latest) return;
      pending.set(snapshot.id, snapshot);
      schedule();
    },
    retire(id: string) {
      if (disposed) return;
      pending.delete(id);
      retiring.add(id);
      schedule();
    },
    receipt(id: string) { return receipts.get(id); },
    async flush() {
      clearTimer();
      const revisions = new Map([...pending].map(([id, snapshot]) => [id, snapshot.revision]));
      if (writing) revisions.set(writing.id, Math.max(revisions.get(writing.id) ?? -1, writing.revision));
      const retirements = [...retiring];
      const incomplete = () => retirements.some(id => retiring.has(id)) || [...revisions].some(([id, revision]) => {
        if ((receipts.get(id)?.revision ?? -1) >= revision) return false;
        return pending.has(id) || writing?.id === id || retiring.has(id);
      });
      // This barrier covers work present at invocation. Continuous later edits
      // cannot postpone a save, background checkpoint or installer forever.
      do {
        if (disposed) throw { code: 'RECOVERY_DISPOSED', message: 'Recovery storage is closed.' };
        await drain();
      } while (incomplete());
      if (disposed) throw { code: 'RECOVERY_DISPOSED', message: 'Recovery storage is closed.' };
    },
    dispose() { disposed = true; clearTimer(); }
  };
}
