import { get, writable } from 'svelte/store';
import { describe, expect, it, vi } from 'vitest';
import { distinctProjection } from './distinctProjection';

describe('distinctProjection', () => {
  it('does not notify consumers when an unrelated source field changes', () => {
    const source = writable({ selected: 'A', largeContent: 'first' });
    const selected = distinctProjection(source, (state) => state.selected);
    const observer = vi.fn();
    const unsubscribe = selected.subscribe(observer);

    source.set({ selected: 'A', largeContent: 'second' });
    source.set({ selected: 'B', largeContent: 'second' });

    expect(get(selected)).toBe('B');
    expect(observer.mock.calls.map(([value]) => value)).toEqual(['A', 'B']);
    unsubscribe();
  });
});
