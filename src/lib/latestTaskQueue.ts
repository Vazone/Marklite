export type LatestTaskResult<Output> =
  | { status: 'completed'; value: Output }
  | { status: 'superseded' }
  | { status: 'disposed' };

type PendingTask<Input, Output> = {
  input: Input;
  resolve: (result: LatestTaskResult<Output>) => void;
  reject: (error: unknown) => void;
};

export type LatestTaskQueue<Input, Output> = {
  submit: (input: Input) => Promise<LatestTaskResult<Output>>;
  dispose: () => void;
};

export function createLatestTaskQueue<Input, Output>(
  execute: (input: Input) => Promise<Output>
): LatestTaskQueue<Input, Output> {
  let pending: PendingTask<Input, Output> | null = null;
  let running = false;
  let disposed = false;

  async function drain(): Promise<void> {
    if (running || disposed) return;
    running = true;
    try {
      while (!disposed && pending) {
        const task = pending;
        pending = null;
        try {
          const value = await execute(task.input);
          task.resolve(disposed ? { status: 'disposed' } : { status: 'completed', value });
        } catch (error) {
          task.reject(error);
        }
      }
    } finally {
      running = false;
      if (!disposed && pending) void drain();
    }
  }

  return {
    submit(input) {
      if (disposed) return Promise.resolve({ status: 'disposed' });
      pending?.resolve({ status: 'superseded' });
      return new Promise<LatestTaskResult<Output>>((resolve, reject) => {
        pending = { input, resolve, reject };
        void drain();
      });
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      pending?.resolve({ status: 'disposed' });
      pending = null;
    }
  };
}
