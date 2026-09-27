import type { ResourceRef } from './platform/resources';

export type AutosaveCandidate = {
  id: string;
  resource: ResourceRef | null;
  dirty: boolean;
  loaded: boolean;
  blocked?: boolean;
};

export type AutosaveScheduler = {
  run: () => Promise<void>;
  dispose: () => void;
};

export function createAutosaveScheduler(
  candidates: () => AutosaveCandidate[],
  save: (id: string, resource: ResourceRef) => Promise<unknown>
): AutosaveScheduler {
  let active: Promise<void> | null = null;
  let disposed = false;

  async function sweep(): Promise<void> {
    const eligible = candidates().filter(
      (candidate): candidate is AutosaveCandidate & { resource: ResourceRef } =>
        candidate.loaded && candidate.dirty && !candidate.blocked && candidate.resource !== null
    );
    for (const candidate of eligible) {
      if (disposed) return;
      await save(candidate.id, candidate.resource);
    }
  }

  return {
    run() {
      if (disposed) return Promise.resolve();
      if (active) return active;
      active = sweep().finally(() => {
        active = null;
      });
      return active;
    },
    dispose() {
      disposed = true;
    }
  };
}
