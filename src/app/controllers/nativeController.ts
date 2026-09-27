import { createLifecycleScope } from '../../lib/lifecycleScope';

type Unlisten = () => void;
type Dependencies = {
  enabled: boolean;
  events: {
    onDrop: (receive: (paths: string[]) => void) => Promise<Unlisten>;
    onOpen: (receive: () => void) => Promise<Unlisten>;
    onClose: (receive: (preventDefault: () => void) => void) => Promise<Unlisten>;
  };
  drain: () => Promise<string[]>;
  open: (path: string) => Promise<unknown>;
  close: (preventDefault: () => void) => void;
  onError: (error: unknown) => void;
  onCloseUnavailable: () => void;
};

/** Owns desktop subscriptions and serializes external file-open requests. */
export function createNativeController(deps: Dependencies) {
  const scope = createLifecycleScope();
  let disposed = false;
  let draining = false;
  let requested = false;
  async function drain() {
    if (disposed) return;
    requested = true;
    if (draining) return;
    draining = true;
    try {
      do {
        requested = false;
        const paths = await deps.drain();
        for (const path of paths) {
          if (disposed) return;
          await deps.open(path);
        }
      } while (requested && !disposed);
    } finally { draining = false; }
  }
  return {
    async dragDrop() {
      if (!deps.enabled || disposed) return true;
      try {
        scope.own(await deps.events.onDrop(paths => {
          if (disposed) return;
          const path = paths.find(path => /\.(md|markdown|txt)$/i.test(path));
          if (path) void deps.open(path).catch(deps.onError);
        }));
        return true;
      } catch (error) { console.warn('Failed to register drag drop handler', error); return false; }
    },
    async externalOpen() {
      if (!deps.enabled || disposed) return true;
      try {
        scope.own(await deps.events.onOpen(() => { void drain().catch(deps.onError); }));
        await drain();
        return true;
      } catch (error) { console.warn('Failed to register external open listener', error); return false; }
    },
    async closeProtection() {
      if (!deps.enabled || disposed) return;
      try {
        scope.own(await deps.events.onClose(prevent => { if (!disposed) deps.close(prevent); }));
      } catch (error) {
        console.warn('Failed to register close protection', error);
        if (!disposed) deps.onCloseUnavailable();
      }
    },
    dispose() { disposed = true; return scope.dispose(); }
  };
}
