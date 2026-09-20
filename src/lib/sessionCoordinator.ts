import type { SessionStateDto } from './tauriApi';

export type SessionPersistIntent = {
  enabled: boolean;
  session: SessionStateDto;
};

export type SessionCoordinatorOptions = {
  persist: (intent: SessionPersistIntent) => Promise<void>;
  onError: (error: unknown) => void;
  delayMs?: number;
};

export function createSessionCoordinator(options: SessionCoordinatorOptions) {
  let writable = false;
  let disposed = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let pending: SessionPersistIntent | undefined;
  let active: Promise<void> | undefined;

  function clearTimer(): void {
    if (timer !== undefined) clearTimeout(timer);
    timer = undefined;
  }

  function drain(): Promise<void> {
    if (active) return active;
    active = (async () => {
      while (!disposed && writable && pending) {
        const intent = pending;
        pending = undefined;
        try {
          await options.persist(intent);
        } catch (error) {
          if (!pending) pending = intent;
          throw error;
        }
      }
    })().finally(() => {
      active = undefined;
    });
    return active;
  }

  function scheduleDrain(): void {
    clearTimer();
    timer = setTimeout(() => {
      timer = undefined;
      void drain().catch(options.onError);
    }, options.delayMs ?? 200);
  }

  return {
    setWritable(next: boolean): void {
      writable = next;
      if (!next) {
        clearTimer();
        pending = undefined;
      }
    },

    queue(intent: SessionPersistIntent): void {
      if (!writable || disposed) return;
      pending = {
        enabled: intent.enabled,
        session: { ...intent.session, paths: [...intent.session.paths] }
      };
      scheduleDrain();
    },

    async flush(): Promise<void> {
      if (!writable || disposed) return;
      clearTimer();
      while (active || pending) {
        if (active) await active;
        else await drain();
      }
    },

    dispose(): void {
      disposed = true;
      writable = false;
      clearTimer();
      pending = undefined;
    }
  };
}
