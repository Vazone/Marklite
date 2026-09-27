import { afterEach, describe, expect, test, vi } from 'vitest';
import { createPaneZoom, observePinch } from './panePinchZoom';

function sendTouch(element: HTMLElement, type: string, points: [number, number][]) {
  const event = new Event(type, { bubbles: true, cancelable: true });
  Object.defineProperty(event, 'touches', {
    value: points.map(([clientX, clientY]) => ({ clientX, clientY, target: element }))
  });
  element.dispatchEvent(event);
  return event;
}

function sendPointer(element: HTMLElement, type: string, x: number, y: number) {
  const event = new MouseEvent(type, { bubbles: true, cancelable: true, button: 0, clientX: x, clientY: y });
  Object.defineProperties(event, { pointerId: { value: 1 }, pointerType: { value: 'mouse' } });
  element.dispatchEvent(event);
  return event;
}

afterEach(() => document.body.replaceChildren());

describe('pane pinch zoom', () => {
  test('consumes only Ctrl+wheel, keeps panes independent, and reverses to one', () => {
    const first = document.createElement('div');
    const second = document.createElement('div');
    const firstSurface = document.createElement('div');
    const secondSurface = document.createElement('div');
    first.append(firstSurface);
    second.append(secondSurface);
    document.body.append(first, second);
    for (const viewport of [first, second]) {
      Object.defineProperties(viewport, {
        clientWidth: { value: 200 }, clientHeight: { value: 300 }
      });
      vi.spyOn(viewport, 'getBoundingClientRect').mockReturnValue({ left: 0, top: 0 } as DOMRect);
    }
    const firstZoom = createPaneZoom(first, firstSurface, undefined, { wheel: true });
    const secondZoom = createPaneZoom(second, secondSurface, undefined, { wheel: true });
    const normal = new WheelEvent('wheel', { bubbles: true, cancelable: true, deltaY: -100 });
    firstSurface.dispatchEvent(normal);
    expect(normal.defaultPrevented).toBe(false);
    expect(firstZoom.scale()).toBe(1);

    const zoomIn = new WheelEvent('wheel', { bubbles: true, cancelable: true, ctrlKey: true,
      deltaY: -100, clientX: 100, clientY: 150 });
    firstSurface.dispatchEvent(zoomIn);
    expect(zoomIn.defaultPrevented).toBe(true);
    expect(firstZoom.scale()).toBeCloseTo(Math.exp(0.2));
    expect(secondZoom.scale()).toBe(1);
    expect(firstSurface.style.transform).toContain('scale(');
    expect(first.style.transform).toBe('');

    firstSurface.dispatchEvent(new WheelEvent('wheel', { bubbles: true, cancelable: true, ctrlKey: true,
      deltaY: 100, clientX: 100, clientY: 150 }));
    expect(firstZoom.scale()).toBe(1);
    expect(firstSurface.style.transform).toBe('');
    firstZoom.dispose();
    secondZoom.dispose();
    const afterDispose = new WheelEvent('wheel', { bubbles: true, cancelable: true, ctrlKey: true, deltaY: -100 });
    firstSurface.dispatchEvent(afterDispose);
    expect(afterDispose.defaultPrevented).toBe(false);
  });

  test('only consumes two touches and detaches all listeners', () => {
    const host = document.createElement('div');
    document.body.append(host);
    const gestures = vi.fn();
    const dispose = observePinch(host, gestures);
    expect(sendTouch(host, 'touchstart', [[10, 10]]).defaultPrevented).toBe(false);
    expect(sendTouch(host, 'touchstart', [[10, 10], [30, 10]]).defaultPrevented).toBe(true);
    expect(sendTouch(host, 'touchmove', [[5, 10], [35, 10]]).defaultPrevented).toBe(true);
    sendTouch(host, 'touchend', [[5, 10]]);
    expect(gestures.mock.calls.map(([gesture]) => gesture.phase)).toEqual(['start', 'move', 'end']);
    expect(gestures.mock.calls[1][0]).toMatchObject({ ratio: 1.5, clientX: 20, clientY: 10 });
    dispose();
    sendTouch(host, 'touchstart', [[10, 10], [30, 10]]);
    expect(gestures).toHaveBeenCalledTimes(3);
  });

  test('zooms around the fingers without changing layout dimensions and returns to one', () => {
    const host = document.createElement('div');
    const surface = document.createElement('div');
    host.append(surface);
    document.body.append(host);
    Object.defineProperties(host, {
      clientWidth: { value: 200 },
      clientHeight: { value: 300 }
    });
    vi.spyOn(host, 'getBoundingClientRect').mockReturnValue({ left: 10, top: 20 } as DOMRect);
    const changed = vi.fn();
    const zoom = createPaneZoom(host, surface, changed);
    sendTouch(host, 'touchstart', [[60, 70], [160, 70]]);
    sendTouch(host, 'touchmove', [[10, 70], [210, 70]]);
    expect(zoom.scale()).toBe(2);
    expect(surface.style.transform).toBe('translate3d(-100px, -50px, 0) scale(2)');
    expect(host.style.transform).toBe('');
    zoom.reset();
    expect(zoom.scale()).toBe(1);
    expect(surface.style.transform).toBe('');
    zoom.dispose();
    expect(changed).toHaveBeenCalledWith(2);
  });

  test('starts a fresh pinch after a scrolled editor removes the previous touch target', () => {
    const host = document.createElement('div');
    const surface = document.createElement('div');
    const oldLine = document.createElement('div');
    surface.append(oldLine);
    host.append(surface);
    document.body.append(host);
    Object.defineProperties(host, {
      clientWidth: { value: 200 },
      clientHeight: { value: 300 }
    });
    vi.spyOn(host, 'getBoundingClientRect').mockReturnValue({ left: 0, top: 0 } as DOMRect);
    const zoom = createPaneZoom(host, surface);

    sendTouch(oldLine, 'touchstart', [[90, 50], [110, 50]]);
    sendTouch(oldLine, 'touchmove', [[70, 50], [130, 50]]);
    expect(zoom.scale()).toBe(3);

    oldLine.remove();
    // The detached target may never report an end to the stable host.
    const newLine = document.createElement('div');
    surface.append(newLine);
    sendTouch(newLine, 'touchstart', [[40, 50], [160, 50]]);
    sendTouch(newLine, 'touchmove', [[85, 50], [115, 50]]);
    expect(zoom.scale()).toBe(1);
    expect(surface.style.transform).toBe('');
    zoom.dispose();
  });

  test('continues receiving moves and the end event from a detached editor line', () => {
    const host = document.createElement('div');
    const surface = document.createElement('div');
    const line = document.createElement('div');
    surface.append(line);
    host.append(surface);
    document.body.append(host);
    const gestures = vi.fn();
    const dispose = observePinch(host, gestures);

    sendTouch(line, 'touchstart', [[90, 50], [110, 50]]);
    line.remove();
    sendTouch(line, 'touchmove', [[70, 50], [130, 50]]);
    sendTouch(line, 'touchend', [[70, 50]]);

    expect(gestures.mock.calls.map(([gesture]) => gesture.phase)).toEqual(['start', 'move', 'end']);
    expect(gestures.mock.calls[1][0].ratio).toBe(3);
    dispose();
  });

  test('pans a zoomed pane with one touch or mouse drag and leaves unzoomed input alone', () => {
    const host = document.createElement('div');
    const surface = document.createElement('div');
    const line = document.createElement('div');
    surface.append(line);
    host.append(surface);
    document.body.append(host);
    Object.defineProperties(host, { clientWidth: { value: 200 }, clientHeight: { value: 300 } });
    vi.spyOn(host, 'getBoundingClientRect').mockReturnValue({ left: 0, top: 0 } as DOMRect);
    const zoom = createPaneZoom(host, surface, undefined, { wheel: true });
    expect(sendTouch(line, 'touchstart', [[100, 100]]).defaultPrevented).toBe(false);
    expect(sendPointer(line, 'pointerdown', 100, 100).defaultPrevented).toBe(false);
    line.dispatchEvent(new WheelEvent('wheel', { bubbles: true, cancelable: true, ctrlKey: true,
      deltaY: -500, clientX: 100, clientY: 150 }));
    expect(zoom.scale()).toBeCloseTo(Math.exp(0.48));
    expect(sendTouch(line, 'touchstart', [[100, 100]]).defaultPrevented).toBe(true);
    sendTouch(line, 'touchmove', [[20, 70]]);
    expect(surface.style.transform).toContain('translate3d(-');
    const afterTouch = surface.style.transform;
    sendTouch(line, 'touchend', []);
    expect(sendPointer(line, 'pointerdown', 100, 100).defaultPrevented).toBe(true);
    sendPointer(host, 'pointermove', 60, 80);
    expect(surface.style.transform).not.toBe(afterTouch);
    sendPointer(host, 'pointerup', 60, 80);
    zoom.reset();
    expect(sendTouch(line, 'touchstart', [[100, 100]]).defaultPrevented).toBe(false);
    zoom.dispose();
  });

  test('pinch midpoint movement changes only scale, while one finger pans afterwards', () => {
    const host = document.createElement('div');
    const surface = document.createElement('div');
    host.append(surface);
    document.body.append(host);
    Object.defineProperties(host, { clientWidth: { value: 200 }, clientHeight: { value: 300 } });
    vi.spyOn(host, 'getBoundingClientRect').mockReturnValue({ left: 0, top: 0 } as DOMRect);
    const zoom = createPaneZoom(host, surface);
    sendTouch(surface, 'touchstart', [[90, 100], [110, 100]]);
    sendTouch(surface, 'touchmove', [[90, 100], [110, 100]]);
    expect(surface.style.transform).toBe('');
    sendTouch(surface, 'touchmove', [[100, 100], [140, 100]]);
    expect(surface.style.transform).toBe('translate3d(-100px, -100px, 0) scale(2)');
    sendTouch(surface, 'touchend', []);
    sendTouch(surface, 'touchstart', [[100, 100]]);
    sendTouch(surface, 'touchmove', [[120, 100]]);
    expect(surface.style.transform).toBe('translate3d(-80px, -100px, 0) scale(2)');
    zoom.dispose();
  });
});
