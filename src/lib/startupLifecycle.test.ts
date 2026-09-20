import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  claimStartupRetry,
  clearStartupRetry,
  confirmInteractiveReady,
  installStartupErrorGuards,
  renderStartupRecovery,
  requireAppRoot,
  startupFailureCode,
  StartupLifecycleError,
  waitForInteractiveFrame
} from './startupLifecycle';

afterEach(() => {
  document.body.replaceChildren();
  sessionStorage.clear();
});

describe('startup lifecycle', () => {
  it('requires an explicit app root without unsafe casting', () => {
    expect(() => requireAppRoot(document)).toThrowError(StartupLifecycleError);
    document.body.innerHTML = '<div id="app"></div>';
    expect(requireAppRoot(document).id).toBe('app');
  });

  it('acks ready only after a frame and a real DOM sentinel', async () => {
    document.body.innerHTML = '<div id="app"><div data-marklite-ready="true"></div></div>';
    const root = requireAppRoot(document);
    const order: string[] = [];
    const ack = vi.fn(async () => {
      order.push('ack');
    });

    await confirmInteractiveReady(
      root,
      ack,
      10,
      (callback) => {
        order.push('frame');
        callback(25);
        return 1;
      }
    );

    expect(order).toEqual(['frame', 'ack']);
    expect(root.querySelector('[data-marklite-ready]')?.getAttribute('data-marklite-interactive')).toBe('true');
    expect(ack).toHaveBeenCalledOnce();
  });

  it('fails deterministically when the browser never produces an interactive frame', async () => {
    vi.useFakeTimers();
    const cancelFrame = vi.fn();
    const pending = waitForInteractiveFrame(() => 17, 50, cancelFrame);
    const rejection = expect(pending).rejects.toMatchObject({ code: 'interactiveFrameTimeout' });

    await vi.advanceTimersByTimeAsync(50);

    await rejection;
    expect(cancelFrame).toHaveBeenCalledWith(17);
    vi.useRealTimers();
  });

  it('rejects a missing sentinel and a failed native handshake', async () => {
    document.body.innerHTML = '<div id="app"></div>';
    const root = requireAppRoot(document);
    const frame = (callback: FrameRequestCallback) => {
      callback(1);
      return 1;
    };
    await expect(confirmInteractiveReady(root, vi.fn(), 0, frame)).rejects.toMatchObject({
      code: 'readySentinelMissing'
    });

    root.innerHTML = '<div data-marklite-ready="true"></div>';
    await expect(
      confirmInteractiveReady(root, async () => Promise.reject(new Error('private path must not surface')), 0, frame)
    ).rejects.toMatchObject({ code: 'readyHandshakeFailed' });
    expect(root.querySelector('[data-marklite-ready]')?.hasAttribute('data-marklite-interactive')).toBe(false);
  });

  it('reports only fixed codes for startup-global failures and stops after ready', () => {
    const report = vi.fn();
    const guards = installStartupErrorGuards(report, performance.now(), window);
    window.dispatchEvent(new ErrorEvent('error', { message: 'C:\\Users\\private\\note.md' }));
    window.dispatchEvent(new PromiseRejectionEvent('unhandledrejection', { promise: Promise.resolve(), reason: 'secret' }));
    expect(report.mock.calls.map(([event]) => event.code)).toEqual(['unhandledError']);
    expect(guards.failureCode()).toBe('unhandledError');
    expect(JSON.stringify(report.mock.calls)).not.toContain('private');
    expect(JSON.stringify(report.mock.calls)).not.toContain('secret');

    guards.stop();
    window.dispatchEvent(new ErrorEvent('error', { message: 'later' }));
    expect(report).toHaveBeenCalledTimes(1);
  });

  it('allows only one reload retry per WebView session', () => {
    expect(claimStartupRetry(sessionStorage)).toBe(true);
    expect(claimStartupRetry(sessionStorage)).toBe(false);
    clearStartupRetry(sessionStorage);
    expect(claimStartupRetry(sessionStorage)).toBe(true);
  });

  it('renders a CSS-independent recovery panel with retry and clear actions', async () => {
    document.body.innerHTML = '<div id="app"><p>booting</p></div>';
    const retry = vi.fn();
    const clear = vi.fn(async () => undefined);
    const panel = renderStartupRecovery(document, 'moduleLoadFailed', retry, clear);

    expect(panel.getAttribute('role')).toBe('alert');
    expect(panel.getAttribute('data-startup-error-code')).toBe('moduleLoadFailed');
    const buttons = panel.querySelectorAll('button');
    buttons[0].click();
    buttons[1].click();
    await Promise.resolve();
    expect(retry).toHaveBeenCalledOnce();
    expect(clear).toHaveBeenCalledOnce();
    expect(panel.textContent).not.toContain('C:\\');
  });

  it('keeps clear diagnostics retryable when the operation fails', async () => {
    document.body.innerHTML = '<div id="app"></div>';
    const clear = vi.fn(async () => Promise.reject(new Error('private failure')));
    const panel = renderStartupRecovery(document, 'moduleLoadFailed', vi.fn(), clear);
    const clearButton = panel.querySelectorAll('button')[1];

    clearButton.click();
    await vi.waitFor(() => expect(clearButton.disabled).toBe(false));

    expect(clearButton.textContent).toBe('Clear failed. Try again.');
    expect(panel.textContent).not.toContain('private failure');
  });

  it('keeps typed lifecycle error codes and otherwise uses the fixed fallback', () => {
    expect(startupFailureCode(new StartupLifecycleError('appRootMissing', 'ignored'), 'moduleLoadFailed')).toBe(
      'appRootMissing'
    );
    expect(startupFailureCode(new Error('C:\\private'), 'svelteMountFailed')).toBe('svelteMountFailed');
  });
});
