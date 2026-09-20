import type { Component } from 'svelte';

export type ComponentModule<Props extends Record<string, unknown>, Exports extends Record<string, unknown>> = {
  default: Component<Props, Exports>;
};

export function createLazyComponent<Props extends Record<string, unknown>, Exports extends Record<string, unknown>>(
  importer: () => Promise<ComponentModule<Props, Exports>>
) {
  let component: Component<Props, Exports> | null = null;
  let loading: Promise<Component<Props, Exports>> | null = null;

  return {
    current() {
      return component;
    },
    load() {
      if (component) return Promise.resolve(component);
      if (!loading) {
        loading = importer()
          .then((module) => {
            component = module.default;
            return component;
          })
          .catch((error: unknown) => {
            // A failed dynamic import must not poison the loader forever. The
            // owning feature decides when a retry is appropriate and remains
            // responsible for presenting the error to the user.
            loading = null;
            throw error;
          });
      }
      return loading;
    }
  };
}
