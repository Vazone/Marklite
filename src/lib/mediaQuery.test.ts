import { describe, expect, it, vi } from 'vitest';
import { observeMediaQuery } from './mediaQuery';

describe('media query observation', () => {
  it('owns the listener and reports the current change value', () => {
    let listener: ((event: MediaQueryListEvent) => void) | undefined;
    const media = {
      addEventListener: vi.fn((_type: string, next: (event: MediaQueryListEvent) => void) => {
        listener = next;
      }),
      removeEventListener: vi.fn()
    } as unknown as MediaQueryList;
    const onChange = vi.fn();

    const dispose = observeMediaQuery(media, onChange);
    listener?.({ matches: true } as MediaQueryListEvent);
    dispose();

    expect(onChange).toHaveBeenCalledWith(true);
    expect(media.removeEventListener).toHaveBeenCalledWith('change', listener);
  });
});
