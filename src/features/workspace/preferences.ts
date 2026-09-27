import type { WorkspacePreferences } from '../../lib/platform/workspace';

/** Serialize writes so a slow old save cannot replace the newest root/expansion state. */
export function createPreferencesWriter(
  save: (value: WorkspacePreferences) => Promise<WorkspacePreferences>,
  onError: (error: unknown) => void
) {
  let revision = 0;
  let pending = Promise.resolve();
  return {
    write(value: WorkspacePreferences) {
      const next = ++revision;
      pending = pending.then(async () => {
        if (next !== revision) return;
        try { await save(value); } catch (error) { onError(error); }
      });
      return pending;
    },
    flush: () => pending
  };
}
