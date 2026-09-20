import { mount, tick, unmount } from 'svelte';
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';
import SplitPaneSeparator from './SplitPaneSeparator.svelte';

let host: HTMLDivElement;
let component: ReturnType<typeof mount> | undefined;
let narrowViewport = false;

class ResizeObserverMock {
  observe = vi.fn();
  disconnect = vi.fn();
}

function pointerEvent(type: string, clientX: number, pointerId = 1) {
  const event = new MouseEvent(type, {
    bubbles: true,
    cancelable: true,
    clientX,
    button: 0
  });
  Object.defineProperties(event, {
    pointerId: { value: pointerId },
    isPrimary: { value: true }
  });
  return event;
}

async function renderSeparator(ratio = 0.5) {
  const onRatioChange = vi.fn();
  const onCommit = vi.fn();
  const onCollapse = vi.fn();
  host = document.createElement('div');
  host.getBoundingClientRect = () =>
    ({ left: 100, right: 1100, top: 0, bottom: 600, width: 1000, height: 600, x: 100, y: 0, toJSON() {} }) as DOMRect;
  document.body.append(host);
  component = mount(SplitPaneSeparator, {
    target: host,
    props: { ratio, onRatioChange, onCommit, onCollapse }
  });
  await tick();
  return {
    separator: host.querySelector<HTMLButtonElement>('[role="separator"]')!,
    onRatioChange,
    onCommit,
    onCollapse
  };
}

beforeEach(() => {
  narrowViewport = false;
  vi.stubGlobal('ResizeObserver', ResizeObserverMock);
  vi.stubGlobal(
    'matchMedia',
    vi.fn(
      () =>
        ({
        matches: narrowViewport,
        media: '(max-width: 920px)',
        onchange: null,
        addEventListener: vi.fn(),
        removeEventListener: vi.fn(),
        addListener: vi.fn(),
        removeListener: vi.fn(),
        dispatchEvent: vi.fn(() => true)
        }) as unknown as MediaQueryList
    )
  );
});

afterEach(async () => {
  if (component) await unmount(component);
  component = undefined;
  host?.remove();
  document.body.classList.remove('split-pane-dragging');
  vi.unstubAllGlobals();
});

describe('SplitPaneSeparator pointer behavior', () => {
  test('previews the left threshold during drag and commits preview only on pointerup', async () => {
    const { separator, onRatioChange, onCommit } = await renderSeparator();

    separator.dispatchEvent(pointerEvent('pointerdown', 600));
    separator.dispatchEvent(pointerEvent('pointermove', 250));
    await vi.waitFor(() => expect(separator.dataset.dropIntent).toBe('preview'));

    expect(onCommit).not.toHaveBeenCalled();
    expect(onRatioChange).not.toHaveBeenCalled();
    expect(Number.parseFloat(separator.style.getPropertyValue('--split-preview-offset'))).toBeCloseTo(-340);
    expect(document.body.classList.contains('split-pane-dragging')).toBe(true);

    separator.dispatchEvent(pointerEvent('pointerup', 250));
    await tick();
    expect(onCommit).toHaveBeenCalledWith(0.15, 0.16);
    expect(onRatioChange).not.toHaveBeenCalled();
    expect(separator.style.getPropertyValue('--split-preview-offset')).toBe('0px');
    expect(separator.dataset.dropIntent).toBe('split');
    expect(document.body.classList.contains('split-pane-dragging')).toBe(false);
  });

  test('uses the inclusive right threshold for edit and restores the start ratio on cancel', async () => {
    const { separator, onRatioChange, onCommit } = await renderSeparator(0.5);

    separator.dispatchEvent(pointerEvent('pointerdown', 600));
    separator.dispatchEvent(pointerEvent('pointermove', 950));
    await vi.waitFor(() => expect(separator.dataset.dropIntent).toBe('edit'));
    separator.dispatchEvent(pointerEvent('pointercancel', 950));
    await tick();

    expect(onCommit).not.toHaveBeenCalled();
    expect(onRatioChange).not.toHaveBeenCalled();
    expect(separator.style.getPropertyValue('--split-preview-offset')).toBe('0px');
    expect(document.body.classList.contains('split-pane-dragging')).toBe(false);
  });

  test('cleans up and restores the start ratio if the window loses focus', async () => {
    const { separator, onRatioChange, onCommit } = await renderSeparator(0.5);

    separator.dispatchEvent(pointerEvent('pointerdown', 600));
    separator.dispatchEvent(pointerEvent('pointermove', 800));
    await vi.waitFor(() => expect(document.body.classList.contains('split-pane-dragging')).toBe(true));
    window.dispatchEvent(new Event('blur'));
    await tick();

    expect(onCommit).not.toHaveBeenCalled();
    expect(onRatioChange).not.toHaveBeenCalled();
    expect(document.body.classList.contains('split-pane-dragging')).toBe(false);
  });

  test('keeps the committed ratio when release synchronously dispatches lostpointercapture', async () => {
    const { separator, onRatioChange, onCommit } = await renderSeparator(0.5);
    Object.defineProperties(separator, {
      setPointerCapture: { value: vi.fn() },
      hasPointerCapture: { value: vi.fn(() => true) },
      releasePointerCapture: {
        value: vi.fn((pointerId: number) => {
          separator.dispatchEvent(pointerEvent('lostpointercapture', 250, pointerId));
        })
      }
    });

    separator.dispatchEvent(pointerEvent('pointerdown', 600, 9));
    separator.dispatchEvent(pointerEvent('pointerup', 250, 9));

    expect(onCommit).toHaveBeenLastCalledWith(0.15, 0.16);
    expect(onRatioChange).not.toHaveBeenCalled();
  });

  test('ignores pointer interaction in the narrow responsive mode', async () => {
    narrowViewport = true;
    const { separator, onRatioChange, onCommit } = await renderSeparator();

    separator.dispatchEvent(pointerEvent('pointerdown', 600));
    separator.dispatchEvent(pointerEvent('pointermove', 250));
    separator.dispatchEvent(pointerEvent('pointerup', 250));

    expect(onRatioChange).not.toHaveBeenCalled();
    expect(onCommit).not.toHaveBeenCalled();
  });
});

describe('SplitPaneSeparator keyboard and ARIA behavior', () => {
  test('exposes pixel-clamped ARIA bounds and supports 5%/Shift+10% arrows', async () => {
    const { separator, onRatioChange } = await renderSeparator();

    expect(separator.getAttribute('aria-valuemin')).toBe('16');
    expect(separator.getAttribute('aria-valuemax')).toBe('84');
    expect(separator.getAttribute('aria-valuenow')).toBe('50');
    separator.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowLeft', bubbles: true }));
    expect(onRatioChange).toHaveBeenLastCalledWith(0.45);
    separator.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'ArrowRight', shiftKey: true, bubbles: true })
    );
    expect(onRatioChange).toHaveBeenLastCalledWith(0.6);
  });

  test('maps Home to preview and End to edit without a pointer gesture', async () => {
    const { separator, onCollapse } = await renderSeparator();

    separator.dispatchEvent(new KeyboardEvent('keydown', { key: 'Home', bubbles: true }));
    separator.dispatchEvent(new KeyboardEvent('keydown', { key: 'End', bubbles: true }));

    expect(onCollapse).toHaveBeenNthCalledWith(1, 'preview');
    expect(onCollapse).toHaveBeenNthCalledWith(2, 'edit');
  });
});
