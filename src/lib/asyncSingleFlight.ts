export type SingleFlightRun<T> = {
  started: boolean;
  promise: Promise<T>;
};

export function createAsyncSingleFlight<Key, Value>() {
  const inFlight = new Map<Key, Promise<Value>>();

  return {
    run(key: Key, operation: () => Promise<Value>): SingleFlightRun<Value> {
      const existing = inFlight.get(key);
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
