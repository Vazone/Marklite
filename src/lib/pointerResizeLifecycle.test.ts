import { afterEach, describe, expect, test, vi } from 'vitest';
import { createPointerResizeLifecycle } from './pointerResizeLifecycle';

function pointerEvent(pointerId = 7): PointerEvent {
  return { pointerId } as PointerEvent;
}

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe('pointer resize lifecycle contract', () => {
  test('ends ownership before release so synchronous lostpointercapture cannot roll back commit', () => {
    const element = document.createElement('div');
    const onCancel = vi.fn();
    Object.defineProperties(element, {
      setPointerCapture: { value: vi.fn() },
      hasPointerCapture: { value: vi.fn(() => true) },
      releasePointerCapture: { value: vi.fn() }
    });
    const lifecycle = createPointerResizeLifecycle({ element, onCancel });
    vi.mocked(element.releasePointerCapture).mockImplementation((pointerId) => {
      lifecycle.handleLostPointerCapture(pointerEvent(pointerId));
    });

    expect(lifecycle.start(pointerEvent())).toBe(true);
    expect(lifecycle.commit(pointerEvent())).toBe(true);

    expect(onCancel).not.toHaveBeenCalled();
    expect(lifecycle.active).toBe(false);
    expect(element.releasePointerCapture).toHaveBeenCalledWith(7);
  });

  test('restores through one cancel callback for pointer cancel, capture loss and blur', () => {
    const element = document.createElement('div');
    const onCancel = vi.fn();
    Object.defineProperties(element, {
      setPointerCapture: { value: vi.fn() },
      hasPointerCapture: { value: vi.fn(() => false) },
      releasePointerCapture: { value: vi.fn() }
    });
    const lifecycle = createPointerResizeLifecycle({ element, onCancel });

    lifecycle.start(pointerEvent(1));
    expect(lifecycle.cancel(pointerEvent(1))).toBe(true);
    lifecycle.start(pointerEvent(2));
    expect(lifecycle.handleLostPointerCapture(pointerEvent(2))).toBe(true);
    lifecycle.start(pointerEvent(3));
    expect(lifecycle.cancelActive()).toBe(true);

    expect(onCancel).toHaveBeenCalledTimes(3);
    expect(lifecycle.active).toBe(false);
  });

  test('coalesces pointer samples to one animation frame and cancels pending work', () => {
    const element = document.createElement('div');
    const onSample = vi.fn();
    let frameCallback: FrameRequestCallback | undefined;
    vi.stubGlobal('requestAnimationFrame', vi.fn((callback: FrameRequestCallback) => {
      frameCallback = callback;
      return 41;
    }));
    vi.stubGlobal('cancelAnimationFrame', vi.fn());
    const lifecycle = createPointerResizeLifecycle({ element, onSample });

    lifecycle.start(pointerEvent());
    lifecycle.schedule(100);
    lifecycle.schedule(240);
    expect(requestAnimationFrame).toHaveBeenCalledOnce();
    frameCallback?.(0);
    expect(onSample).toHaveBeenCalledOnce();
    expect(onSample).toHaveBeenCalledWith(240);

    lifecycle.schedule(300);
    lifecycle.cancelActive();
    expect(cancelAnimationFrame).toHaveBeenCalledWith(41);
  });
});
