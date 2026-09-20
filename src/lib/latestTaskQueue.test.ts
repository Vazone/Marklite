import { describe, expect, it, vi } from 'vitest';
import { createLatestTaskQueue } from './latestTaskQueue';

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

describe('latest task queue', () => {
  it('runs one task at a time and retains only the newest pending input', async () => {
    const first = deferred<string>();
    const execute = vi.fn((value: string) => (value === 'first' ? first.promise : Promise.resolve(value)));
    const queue = createLatestTaskQueue(execute);

    const firstResult = queue.submit('first');
    const superseded = queue.submit('second');
    const latest = queue.submit('latest');
    expect(execute).toHaveBeenCalledTimes(1);
    await expect(superseded).resolves.toEqual({ status: 'superseded' });

    first.resolve('first');
    await expect(firstResult).resolves.toEqual({ status: 'completed', value: 'first' });
    await expect(latest).resolves.toEqual({ status: 'completed', value: 'latest' });
    expect(execute.mock.calls.map(([value]) => value)).toEqual(['first', 'latest']);
  });

  it('settles pending and active ownership when disposed', async () => {
    const active = deferred<string>();
    const queue = createLatestTaskQueue(() => active.promise);
    const activeResult = queue.submit('active');
    const pendingResult = queue.submit('pending');

    queue.dispose();
    await expect(pendingResult).resolves.toEqual({ status: 'disposed' });
    active.resolve('late');
    await expect(activeResult).resolves.toEqual({ status: 'disposed' });
    await expect(queue.submit('after')).resolves.toEqual({ status: 'disposed' });
  });

  it('propagates execution errors and continues with the latest pending task', async () => {
    const failed = deferred<string>();
    const queue = createLatestTaskQueue((value: string) =>
      value === 'failed' ? failed.promise : Promise.resolve(value)
    );
    const failure = queue.submit('failed');
    const next = queue.submit('next');
    failed.reject(new Error('failed task'));

    await expect(failure).rejects.toThrow('failed task');
    await expect(next).resolves.toEqual({ status: 'completed', value: 'next' });
  });
});
