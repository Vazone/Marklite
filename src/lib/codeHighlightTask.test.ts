import { afterEach, expect, test, vi } from 'vitest';
import { highlightCodeTask } from './codeHighlightTask';

afterEach(() => vi.useRealTimers());
function worker() {
  return { onmessage: null as ((event: { data: unknown }) => void) | null, onerror: null,
    postMessage: vi.fn(), terminate: vi.fn() };
}

test('loads before parsing and terminates after delivering tokens', async () => {
  const w = worker();
  const task = highlightCodeTask('const x = 1', 'js', new AbortController().signal, () => w as unknown as Worker);
  expect(w.postMessage).toHaveBeenLastCalledWith({ language: 'js' });
  w.onmessage!({ data: { ready: true } });
  expect(w.postMessage).toHaveBeenLastCalledWith({ source: 'const x = 1' });
  const tokens = [{ from: 0, to: 5, classes: 'tok-keyword' }];
  w.onmessage!({ data: { tokens } });
  await expect(task).resolves.toEqual(tokens);
  expect(w.terminate).toHaveBeenCalledOnce();
});

test.each(['loading', 'parsing'])('abort during %s prevents late results', async phase => {
  const w = worker(), abort = new AbortController();
  const task = highlightCodeTask('let x', 'js', abort.signal, () => w as unknown as Worker);
  if (phase === 'parsing') w.onmessage!({ data: { ready: true } });
  abort.abort();
  w.onmessage!({ data: { tokens: [{ from: 0, to: 3, classes: 'tok-keyword' }] } });
  await expect(task).resolves.toEqual([]);
  expect(w.terminate).toHaveBeenCalledOnce();
});

test('watchdog terminates a parser that cannot yield', async () => {
  vi.useFakeTimers();
  const w = worker();
  const task = highlightCodeTask('let x', 'js', new AbortController().signal, () => w as unknown as Worker);
  await vi.advanceTimersByTimeAsync(1000);
  expect(w.terminate).not.toHaveBeenCalled();
  w.onmessage!({ data: { ready: true } });
  await vi.advanceTimersByTimeAsync(100);
  await expect(task).resolves.toEqual([]);
  expect(w.terminate).toHaveBeenCalledOnce();
});

test('unsupported or oversized input does not allocate a worker', async () => {
  const create = vi.fn();
  await expect(highlightCodeTask('text', 'unknown', new AbortController().signal, create)).resolves.toEqual([]);
  await expect(highlightCodeTask('x'.repeat(20000), 'js', new AbortController().signal, create)).resolves.toEqual([]);
  expect(create).not.toHaveBeenCalled();
});
