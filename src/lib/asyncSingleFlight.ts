export type SingleFlightRun<T> = {
  started: boolean;
  promise: Promise<T>;
};

export function createAsyncSingleFlight<Key>() {
  const inFlight = new Map<Key, Promise<unknown>>();

  return {
    has(key: Key): boolean {
      return inFlight.has(key);
    },
    run<T>(key: Key, operation: () => Promise<T>): SingleFlightRun<T> {
      const existing = inFlight.get(key) as Promise<T> | undefined;
      if (existing) {
        return { started: false, promise: existing };
      }

      const promise = Promise.resolve()
        .then(operation)
        .finally(() => {
          if (inFlight.get(key) === promise) {
            inFlight.delete(key);
          }
        });
      inFlight.set(key, promise);
      return { started: true, promise };
    }
  };
}
