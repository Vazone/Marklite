export type DisposeResource = () => void;

export type LifecycleScope = {
  own: (dispose: DisposeResource) => boolean;
  dispose: () => unknown[];
};

export function createLifecycleScope(): LifecycleScope {
  const resources = new Set<DisposeResource>();
  let disposed = false;

  return {
    own(dispose) {
      if (disposed) {
        dispose();
        return false;
      }
      resources.add(dispose);
      return true;
    },
    dispose() {
      if (disposed) return [];
      disposed = true;
      const errors: unknown[] = [];
      for (const dispose of resources) {
        try {
          dispose();
        } catch (error) {
          errors.push(error);
        }
      }
      resources.clear();
      return errors;
    }
  };
}
