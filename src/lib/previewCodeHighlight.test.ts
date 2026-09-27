import { afterEach, expect, test, vi } from 'vitest';
import { createCodeHighlightCache } from './codeHighlightCache';
import { createPreviewCodeHighlight, paintCode } from './previewCodeHighlight';

afterEach(() => { vi.useRealTimers(); document.body.replaceChildren(); });

test('painting hostile code creates text and preserves Unicode and CRLF', () => {
  const element = document.createElement('code');
  const source = '<img src=x onerror="alert(1)">\r\n你好😀';
  paintCode(element, source, [{ from: 0, to: 4, classes: 'tok-string' }]);
  expect(element.textContent).toBe(source);
  expect(element.querySelector('img')).toBeNull();
  expect(element.querySelectorAll('span')).toHaveLength(1);
});

test('cache respects entry and memory limits and reuses exact text', async () => {
  const run = vi.fn(async () => [{ from: 0, to: 1, classes: 'tok-keyword' }]);
  const cache = createCodeHighlightCache(run);
  const signal = new AbortController().signal;
  await cache.get('one', 'js', signal);
  await cache.get('one', 'js', signal);
  expect(run).toHaveBeenCalledTimes(1);
  for (let i = 0; i < 160; i++) await cache.get(`${i}${'a'.repeat(20000)}`, 'js', signal);
  expect(cache.usage().entries).toBeLessThanOrEqual(128);
  expect(cache.usage().bytes).toBeLessThanOrEqual(1024 * 1024);
  cache.clear();
  expect(cache.usage()).toEqual({ entries: 0, bytes: 0 });
});

function fixture(count: number) {
  const host = document.createElement('main');
  host.innerHTML = '<pre><code class="language-js">const a = 1</code></pre>'.repeat(count);
  document.body.append(host);
  host.getBoundingClientRect = () => ({ top: 0, bottom: 100, height: 100 }) as DOMRect;
  return host;
}

test('window scheduling is bounded, serial and cached across repeated updates', async () => {
  vi.useFakeTimers();
  const run = vi.fn(async () => [{ from: 0, to: 5, classes: 'tok-keyword' }]);
  const service = createPreviewCodeHighlight(createCodeHighlightCache(run));
  const host = fixture(100);
  service.update(host);
  await vi.advanceTimersByTimeAsync(32);
  expect(host.querySelectorAll('span')).toHaveLength(64);
  expect(run).toHaveBeenCalledTimes(1);
  service.update(host);
  await vi.advanceTimersByTimeAsync(32);
  expect(run).toHaveBeenCalledTimes(1);
  service.dispose();
});

test('old window results cannot decorate a detached or replaced document', async () => {
  vi.useFakeTimers();
  let resolve!: (tokens: { from: number; to: number; classes: string }[]) => void;
  let signal!: AbortSignal;
  const cache = createCodeHighlightCache(async (_source, _language, abort) => {
    signal = abort;
    return new Promise(done => { resolve = done; });
  });
  const service = createPreviewCodeHighlight(cache);
  const host = fixture(1), oldCode = host.querySelector('code')!;
  service.update(host);
  await vi.advanceTimersByTimeAsync(32);
  host.replaceChildren();
  service.update(host);
  expect(signal.aborted).toBe(true);
  resolve([{ from: 0, to: 5, classes: 'tok-keyword' }]);
  await vi.advanceTimersByTimeAsync(32);
  expect(oldCode.querySelector('span')).toBeNull();
  expect(cache.usage().entries).toBe(0);
  service.dispose();
});
