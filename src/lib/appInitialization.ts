export class AppInitializationError extends Error {
  constructor() {
    super('Application initialization did not complete.');
    this.name = 'AppInitializationError';
  }
}

export type AppInitializationGate = {
  wait: () => Promise<void>;
  succeed: () => void;
  fail: () => void;
};

export function createAppInitializationGate(): AppInitializationGate {
  let state: 'pending' | 'succeeded' | 'failed' = 'pending';
  let resolveGate!: () => void;
  let rejectGate!: (error: AppInitializationError) => void;
  const completion = new Promise<void>((resolve, reject) => {
    resolveGate = resolve;
    rejectGate = reject;
  });

  return {
    wait() {
      return completion;
    },
    succeed() {
      if (state !== 'pending') return;
      state = 'succeeded';
      resolveGate();
    },
    fail() {
      if (state !== 'pending') return;
      state = 'failed';
      rejectGate(new AppInitializationError());
    }
  };
}

export const appInitialization = createAppInitializationGate();
