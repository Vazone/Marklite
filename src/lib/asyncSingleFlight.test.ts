import { describe, expect, test, vi } from 'vitest';
import { createAsyncSingleFlight } from './asyncSingleFlight';

describe('async single-flight', () => {
  test('prevents overlapping autosave operations for one tab and allows the next interval after settlement', async () => {
    let resolveFirst!: (value: string) => void;
    const firstOperation = vi.fn(
      () =>
        new Promise<string>((resolve) => {
          resolveFirst = resolve;
        })
    );
    const ignoredOverlap = vi.fn(async () => 'overlap');
    const coordinator = createAsyncSingleFlight<string, string>();

    const first = coordinator.run('tab-1', firstOperation);
    const joined = coordinator.run('tab-1', ignoredOverlap);
    expect(first.started).toBe(true);
    expect(joined.started).toBe(false);
    expect(joined.promise).toBe(first.promise);
    await Promise.resolve();
    resolveFirst('saved');
    await expect(joined.promise).resolves.toBe('saved');
    expect(firstOperation).toHaveBeenCalledTimes(1);
    expect(ignoredOverlap).not.toHaveBeenCalled();
    const next = coordinator.run('tab-1', async () => 'next');
    expect(next.started).toBe(true);
    await expect(next.promise).resolves.toBe('next');
  });

  test('does not serialize unrelated keys', async () => {
    const coordinator = createAsyncSingleFlight<string, string>();
    const first = coordinator.run('tab-1', async () => 'first');
    const second = coordinator.run('tab-2', async () => 'second');

    expect(first.started).toBe(true);
    expect(second.started).toBe(true);
    await expect(Promise.all([first.promise, second.promise])).resolves.toEqual(['first', 'second']);
  });

  test('releases a failed operation so the same key can be retried', async () => {
    const coordinator = createAsyncSingleFlight<string, string>();
    const failed = coordinator.run('tab-1', async () => Promise.reject(new Error('write failed')));

    await expect(failed.promise).rejects.toThrow('write failed');
    await expect(coordinator.run('tab-1', async () => 'retried').promise).resolves.toBe('retried');
  });
});
