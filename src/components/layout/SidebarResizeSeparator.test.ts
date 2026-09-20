import { mount, tick, unmount } from 'svelte';
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';
import SidebarResizeSeparator from './SidebarResizeSeparator.svelte';

let host: HTMLDivElement;
let component: ReturnType<typeof mount> | undefined;

class ResizeObserverMock {
  observe = vi.fn();
  disconnect = vi.fn();
}

function pointerEvent(type: string, clientX: number, pointerId = 1): Event {
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

async function renderSeparator(width = 280) {
  const onWidthChange = vi.fn();
  const onCommit = vi.fn();
  const onCollapse = vi.fn();
  host = document.createElement('div');
  host.getBoundingClientRect = () =>
    ({ left: 100, right: 1100, top: 0, bottom: 600, width: 1000, height: 600, x: 100, y: 0, toJSON() {} }) as DOMRect;
  document.body.append(host);
  component = mount(SidebarResizeSeparator, {
    target: host,
    props: { width, onWidthChange, onCommit, onCollapse }
  });
  await tick();
  return {
    separator: host.querySelector<HTMLElement>('[role="separator"]')!,
    onWidthChange,
    onCommit,
    onCollapse
  };
}

beforeEach(() => {
  vi.stubGlobal('ResizeObserver', ResizeObserverMock);
  vi.stubGlobal(
    'matchMedia',
    vi.fn(
      () =>
        ({
          matches: false,
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
  document.body.classList.remove('sidebar-resize-dragging');
  vi.unstubAllGlobals();
});

describe('SidebarResizeSeparator', () => {
  test('shows collapse intent during drag but commits only on pointerup', async () => {
    const { separator, onWidthChange, onCommit } = await renderSeparator();
    separator.dispatchEvent(pointerEvent('pointerdown', 380));
    separator.dispatchEvent(pointerEvent('pointermove', 250));
    await vi.waitFor(() => expect(separator.classList.contains('collapse-intent')).toBe(true));

    expect(onCommit).not.toHaveBeenCalled();
    expect(onWidthChange).toHaveBeenCalledWith(150);
    separator.dispatchEvent(pointerEvent('pointerup', 260));
    expect(onCommit).toHaveBeenCalledWith(160, 160, 1000);
    expect(document.body.classList.contains('sidebar-resize-dragging')).toBe(false);
  });

  test('restores the start width on cancel and supports keyboard sizing', async () => {
    const { separator, onWidthChange, onCommit, onCollapse } = await renderSeparator(280);
    separator.dispatchEvent(pointerEvent('pointerdown', 380));
    separator.dispatchEvent(pointerEvent('pointermove', 520));
    await vi.waitFor(() => expect(onWidthChange).toHaveBeenCalled());
    separator.dispatchEvent(pointerEvent('pointercancel', 520));
    expect(onWidthChange).toHaveBeenLastCalledWith(280);
    expect(onCommit).not.toHaveBeenCalled();

    separator.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }));
    expect(onCommit).toHaveBeenLastCalledWith(296, 296, 1000);
    separator.dispatchEvent(new KeyboardEvent('keydown', { key: 'Home', bubbles: true }));
    expect(onCollapse).toHaveBeenCalledOnce();
    separator.dispatchEvent(new KeyboardEvent('keydown', { key: 'End', bubbles: true }));
    expect(onCommit).toHaveBeenLastCalledWith(450, 450, 1000);
  });

  test('keeps the committed width when release dispatches lostpointercapture', async () => {
    const { separator, onWidthChange, onCommit } = await renderSeparator(280);
    Object.defineProperties(separator, {
      setPointerCapture: { value: vi.fn() },
      hasPointerCapture: { value: vi.fn(() => true) },
      releasePointerCapture: {
        value: vi.fn((pointerId: number) => {
          separator.dispatchEvent(pointerEvent('lostpointercapture', 460, pointerId));
        })
      }
    });

    separator.dispatchEvent(pointerEvent('pointerdown', 380, 9));
    separator.dispatchEvent(pointerEvent('pointerup', 460, 9));

    expect(onCommit).toHaveBeenLastCalledWith(360, 360, 1000);
    expect(onWidthChange).toHaveBeenLastCalledWith(360);
  });

  test('exposes pixel ARIA bounds derived from the workspace', async () => {
    const { separator } = await renderSeparator(280);
    expect(separator.getAttribute('aria-valuemin')).toBe('200');
    expect(separator.getAttribute('aria-valuemax')).toBe('450');
    expect(separator.getAttribute('aria-valuenow')).toBe('280');
  });
});
