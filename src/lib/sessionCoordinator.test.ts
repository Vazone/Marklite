import { describe, expect, test, vi } from 'vitest';
import { createSessionCoordinator, type SessionPersistIntent } from './sessionCoordinator';

function intent(activePath: string | null): SessionPersistIntent {
  return {
    enabled: true,
    session: { version: 1, paths: activePath ? [activePath] : [], activePath }
  };
}

describe('session persistence coordinator', () => {
  test('never writes when restore did not grant write permission', async () => {
    vi.useFakeTimers();
    const persist = vi.fn(async () => undefined);
    const coordinator = createSessionCoordinator({ persist, onError: vi.fn() });

    coordinator.queue(intent('C:\\future.md'));
    await vi.advanceTimersByTimeAsync(500);
    await coordinator.flush();

    expect(persist).not.toHaveBeenCalled();
    vi.useRealTimers();
  });

  test('debounces to one latest immutable snapshot', async () => {
    vi.useFakeTimers();
    const persist = vi.fn(async () => undefined);
    const coordinator = createSessionCoordinator({ persist, onError: vi.fn() });
    coordinator.setWritable(true);
    const first = intent('C:\\first.md');
    coordinator.queue(first);
    first.session.paths[0] = 'C:\\mutated.md';
    coordinator.queue(intent('C:\\latest.md'));

    await vi.advanceTimersByTimeAsync(200);

    expect(persist).toHaveBeenCalledOnce();
    expect(persist).toHaveBeenCalledWith(intent('C:\\latest.md'));
    vi.useRealTimers();
  });

  test('flush cancels debounce and waits for the final queued snapshot', async () => {
    vi.useFakeTimers();
    let finish!: () => void;
    const persist = vi.fn(() => new Promise<void>((resolve) => { finish = resolve; }));
    const coordinator = createSessionCoordinator({ persist, onError: vi.fn() });
    coordinator.setWritable(true);
    coordinator.queue(intent('C:\\active.md'));

    const flushing = coordinator.flush();
    await Promise.resolve();
    expect(persist).toHaveBeenCalledOnce();
    finish();
    await expect(flushing).resolves.toBeUndefined();
    expect(vi.getTimerCount()).toBe(0);
    vi.useRealTimers();
  });

  test('serializes a newer intent queued while a write is active', async () => {
    let finishFirst!: () => void;
    const persist = vi.fn()
      .mockImplementationOnce(() => new Promise<void>((resolve) => { finishFirst = resolve; }))
      .mockResolvedValue(undefined);
    const coordinator = createSessionCoordinator({ persist, onError: vi.fn(), delayMs: 0 });
    coordinator.setWritable(true);
    coordinator.queue(intent('C:\\first.md'));
    await new Promise((resolve) => setTimeout(resolve, 0));
    coordinator.queue(intent('C:\\latest.md'));
    const flushing = coordinator.flush();
    finishFirst();
    await flushing;

    expect(persist.mock.calls.map(([value]) => value.session.activePath)).toEqual([
      'C:\\first.md', 'C:\\latest.md'
    ]);
  });

  test('reports background failure, retains the intent, and makes a failing exit flush observable', async () => {
    vi.useFakeTimers();
    const failure = new Error('write failed');
    const onError = vi.fn();
    const persist = vi.fn().mockRejectedValue(failure);
    const coordinator = createSessionCoordinator({ persist, onError });
    coordinator.setWritable(true);
    coordinator.queue(intent('C:\\active.md'));

    await vi.advanceTimersByTimeAsync(200);
    expect(onError).toHaveBeenCalledWith(failure);
    await expect(coordinator.flush()).rejects.toThrow('write failed');
    expect(persist).toHaveBeenCalledTimes(2);
    vi.useRealTimers();
  });

  test('dispose drops a queued newer intent and ignores a late active completion', async () => {
    let finish!: () => void;
    const persist = vi.fn(() => new Promise<void>((resolve) => { finish = resolve; }));
    const coordinator = createSessionCoordinator({ persist, onError: vi.fn(), delayMs: 0 });
    coordinator.setWritable(true);
    coordinator.queue(intent('C:\\active.md'));
    await new Promise((resolve) => setTimeout(resolve, 0));
    coordinator.queue(intent('C:\\late.md'));

    coordinator.dispose();
    finish();
    await Promise.resolve();

    expect(persist).toHaveBeenCalledOnce();
  });
});
