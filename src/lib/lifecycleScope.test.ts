import { describe, expect, it, vi } from 'vitest';
import { createLifecycleScope } from './lifecycleScope';

describe('lifecycle scope', () => {
  it('immediately releases a resource that resolves after the owner is gone', () => {
    const scope = createLifecycleScope();
    const lateDispose = vi.fn();

    scope.dispose();

    expect(scope.own(lateDispose)).toBe(false);
    expect(lateDispose).toHaveBeenCalledOnce();
  });

  it('releases each owned resource once and continues after one cleanup fails', () => {
    const scope = createLifecycleScope();
    const first = vi.fn(() => {
      throw new Error('cleanup failed');
    });
    const second = vi.fn();
    scope.own(first);
    scope.own(second);

    expect(scope.dispose()).toHaveLength(1);
    expect(scope.dispose()).toEqual([]);
    expect(first).toHaveBeenCalledOnce();
    expect(second).toHaveBeenCalledOnce();
  });
});
