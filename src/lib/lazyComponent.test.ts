import { describe, expect, test, vi } from 'vitest';
import type { Component } from 'svelte';
import { createLazyComponent } from './lazyComponent';

describe('lazy component loader', () => {
  test('imports once across concurrent and later feature requests', async () => {
    const component = (() => undefined) as unknown as Component<Record<string, unknown>>;
    const importer = vi.fn(async () => ({ default: component }));
    const lazy = createLazyComponent(importer);

    expect(lazy.current()).toBeNull();
    const first = lazy.load();
    const second = lazy.load();
    await expect(Promise.all([first, second])).resolves.toEqual([component, component]);
    await expect(lazy.load()).resolves.toBe(component);
    expect(lazy.current()).toBe(component);
    expect(importer).toHaveBeenCalledOnce();
  });

  test('does not import before the owning feature asks for the component', () => {
    const importer = vi.fn(async () => ({
      default: (() => undefined) as unknown as Component<Record<string, unknown>>
    }));
    const lazy = createLazyComponent(importer);

    expect(lazy.current()).toBeNull();
    expect(importer).not.toHaveBeenCalled();
  });

  test('shares one rejection and permits an explicit retry', async () => {
    const component = (() => undefined) as unknown as Component<Record<string, unknown>>;
    const importer = vi
      .fn<() => Promise<{ default: typeof component }>>()
      .mockRejectedValueOnce(new Error('chunk unavailable'))
      .mockResolvedValueOnce({ default: component });
    const lazy = createLazyComponent(importer);

    const first = lazy.load();
    const concurrent = lazy.load();
    await expect(first).rejects.toThrow('chunk unavailable');
    await expect(concurrent).rejects.toThrow('chunk unavailable');
    expect(importer).toHaveBeenCalledOnce();
    expect(lazy.current()).toBeNull();

    await expect(lazy.load()).resolves.toBe(component);
    expect(importer).toHaveBeenCalledTimes(2);
    expect(lazy.current()).toBe(component);
  });
});
