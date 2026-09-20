import { afterEach, describe, expect, test, vi } from 'vitest';
import { createMarkdownRenderCoordinator } from './markdownRenderCoordinator';
import type { MarkdownAnalysisDto, RenderedMarkdownDto } from './tauriApi';

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}

const rendered = {} as RenderedMarkdownDto;
const analysis = {} as MarkdownAnalysisDto;

afterEach(() => vi.useRealTimers());

describe('markdown render coordinator', () => {
  test('switching documents and modes keeps only the latest scheduled intent', async () => {
    vi.useFakeTimers();
    const render = vi.fn(async () => rendered);
    const analyze = vi.fn(async () => analysis);
    const onRendered = vi.fn(() => true);
    const onAnalyzed = vi.fn((_request: { tabId: string; contentRevision: number }) => true);
    const coordinator = createMarkdownRenderCoordinator({ render, analyze, onRendered, onAnalyzed, onError: vi.fn() });

    coordinator.update({ tabId: 'a', content: 'old', contentRevision: 1 }, 'render', false, 200);
    vi.advanceTimersByTime(100);
    coordinator.update({ tabId: 'b', content: 'new', contentRevision: 2 }, 'analysis', false, 200);
    await vi.advanceTimersByTimeAsync(200);
    expect(render).not.toHaveBeenCalled();
    expect(analyze).toHaveBeenCalledExactlyOnceWith('new');
    expect(onAnalyzed.mock.calls[0][0]).toMatchObject({ tabId: 'b', contentRevision: 2 });

    coordinator.update({ tabId: 'b', content: 'new', contentRevision: 2 }, 'analysis', true, 200);
    await vi.advanceTimersByTimeAsync(200);
    expect(analyze).toHaveBeenCalledTimes(1);
    coordinator.update({ tabId: 'b', content: 'new', contentRevision: 2 }, 'analysis', false, 200);
    await vi.advanceTimersByTimeAsync(200);
    expect(analyze).toHaveBeenCalledTimes(2);
    coordinator.update({ tabId: 'b', content: 'new', contentRevision: 2 }, 'render', true, 200);
    await vi.advanceTimersByTimeAsync(200);
    expect(render).toHaveBeenCalledExactlyOnceWith('new', 'b', 2);
    coordinator.dispose();
  });

  test('a reloaded revision rejects the old result while the newest request completes', async () => {
    vi.useFakeTimers();
    const first = deferred<RenderedMarkdownDto>();
    const render = vi.fn((content: string) => content === 'old' ? first.promise : Promise.resolve(rendered));
    let currentRevision = 1;
    const applied: number[] = [];
    const coordinator = createMarkdownRenderCoordinator({
      render,
      analyze: async () => analysis,
      onRendered: (request) => {
        if (request.contentRevision !== currentRevision) return false;
        applied.push(request.contentRevision);
        return true;
      },
      onAnalyzed: () => true,
      onError: vi.fn()
    });

    const stale = coordinator.runNow({ kind: 'render', tabId: 'a', content: 'old', contentRevision: 1 });
    currentRevision = 2;
    coordinator.update({ tabId: 'a', content: 'new', contentRevision: 2 }, 'render', false, 100);
    vi.advanceTimersByTime(100);
    first.resolve(rendered);
    await expect(stale).resolves.toBe(false);
    await vi.waitFor(() => expect(applied).toEqual([2]));
    expect(render.mock.calls.map(([content]) => content)).toEqual(['old', 'new']);
    coordinator.dispose();
  });

  test('destroy cancels timers and suppresses late success, failure, and UI callbacks', async () => {
    vi.useFakeTimers();
    const lateSuccess = deferred<RenderedMarkdownDto>();
    const lateFailure = deferred<MarkdownAnalysisDto>();
    const onRendered = vi.fn(() => true);
    const onAnalyzed = vi.fn(() => true);
    const onError = vi.fn();
    const successCoordinator = createMarkdownRenderCoordinator({
      render: () => lateSuccess.promise,
      analyze: async () => analysis,
      onRendered, onAnalyzed, onError
    });
    const success = successCoordinator.runNow({ kind: 'render', tabId: 'a', content: 'a', contentRevision: 1 });
    successCoordinator.update({ tabId: 'b', content: 'b', contentRevision: 1 }, 'render', false, 100);
    successCoordinator.dispose();
    await vi.advanceTimersByTimeAsync(100);
    lateSuccess.resolve(rendered);
    await expect(success).resolves.toBe(false);
    expect(onRendered).not.toHaveBeenCalled();

    const failureCoordinator = createMarkdownRenderCoordinator({
      render: async () => rendered,
      analyze: () => lateFailure.promise,
      onRendered, onAnalyzed, onError
    });
    const failure = failureCoordinator.runNow({ kind: 'analysis', tabId: 'a', content: 'a', contentRevision: 1 });
    failureCoordinator.dispose();
    lateFailure.reject(new Error('late'));
    await expect(failure).resolves.toBe(false);
    expect(onAnalyzed).not.toHaveBeenCalled();
    expect(onError).not.toHaveBeenCalled();
  });
});
