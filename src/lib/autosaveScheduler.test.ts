import { describe, expect, test, vi } from 'vitest';
import { createAutosaveScheduler, type AutosaveCandidate } from './autosaveScheduler';

function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((complete) => (resolve = complete));
  return { promise, resolve };
}

describe('autosave scheduler', () => {
  test('saves every loaded named dirty tab and skips ineligible tabs', async () => {
    const candidates: AutosaveCandidate[] = [
      { id: 'a', path: 'C:\\a.md', dirty: true, loaded: true },
      { id: 'b', path: 'C:\\b.md', dirty: true, loaded: true },
      { id: 'clean', path: 'C:\\clean.md', dirty: false, loaded: true },
      { id: 'unnamed', path: null, dirty: true, loaded: true },
      { id: 'lazy', path: 'C:\\lazy.md', dirty: true, loaded: false },
      { id: 'conflicted', path: 'C:\\conflicted.md', dirty: true, loaded: true, blocked: true }
    ];
    const save = vi.fn(async () => undefined);
    const scheduler = createAutosaveScheduler(() => candidates, save);

    await scheduler.run();

    expect(save.mock.calls).toEqual([
      ['a', 'C:\\a.md'],
      ['b', 'C:\\b.md']
    ]);
  });

  test('coalesces overlapping timer ticks and stops starting work after disposal', async () => {
    const first = deferred();
    const save = vi.fn(() => first.promise);
    const scheduler = createAutosaveScheduler(
      () => [{ id: 'a', path: 'C:\\a.md', dirty: true, loaded: true }],
      save
    );

    const one = scheduler.run();
    const two = scheduler.run();
    expect(one).toBe(two);
    expect(save).toHaveBeenCalledOnce();
    scheduler.dispose();
    first.resolve();
    await one;
    await scheduler.run();
    expect(save).toHaveBeenCalledOnce();
  });
});
