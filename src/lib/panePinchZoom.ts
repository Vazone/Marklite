export type PinchGesture = {
  phase: 'start' | 'move' | 'end';
  ratio: number;
  clientX: number;
  clientY: number;
};

type Point = { x: number; y: number };

function touchCenter(touches: TouchList): Point {
  return {
    x: (touches[0].clientX + touches[1].clientX) / 2,
    y: (touches[0].clientY + touches[1].clientY) / 2
  };
}

function touchDistance(touches: TouchList): number {
  return Math.hypot(touches[0].clientX - touches[1].clientX, touches[0].clientY - touches[1].clientY);
}

export function observePinch(element: HTMLElement, onGesture: (gesture: PinchGesture) => void): () => void {
  let active = false;
  let startDistance = 1;
  const touchTargets = new Set<EventTarget>();
  const handledMoves = new WeakSet<TouchEvent>();

  const releaseTargets = () => {
    for (const target of touchTargets) {
      target.removeEventListener('touchmove', move as EventListener);
      target.removeEventListener('touchend', end as EventListener);
      target.removeEventListener('touchcancel', end as EventListener);
    }
    touchTargets.clear();
  };

  const finish = () => {
    if (!active) return;
    active = false;
    releaseTargets();
    onGesture({ phase: 'end', ratio: 1, clientX: 0, clientY: 0 });
  };

  const trackTargets = (touches: TouchList) => {
    for (const touch of touches) {
      const target = touch.target;
      if (!target || target === element || touchTargets.has(target)) continue;
      touchTargets.add(target);
      target.addEventListener('touchmove', move as EventListener, { passive: false });
      target.addEventListener('touchend', end as EventListener);
      target.addEventListener('touchcancel', end as EventListener);
    }
  };

  const start = (event: TouchEvent) => {
    if (event.touches.length !== 2) {
      finish();
      return;
    }
    // A virtualized editor may remove the original touch target before its end event reaches this element.
    finish();
    active = true;
    startDistance = Math.max(1, touchDistance(event.touches));
    const center = touchCenter(event.touches);
    onGesture({ phase: 'start', ratio: 1, clientX: center.x, clientY: center.y });
    trackTargets(event.touches);
    event.preventDefault();
  };
  const move = (event: TouchEvent) => {
    if (handledMoves.has(event)) return;
    handledMoves.add(event);
    if (!active) return;
    if (event.touches.length !== 2) {
      finish();
      return;
    }
    const center = touchCenter(event.touches);
    onGesture({ phase: 'move', ratio: touchDistance(event.touches) / startDistance, clientX: center.x, clientY: center.y });
    event.preventDefault();
  };
  const end = (event: TouchEvent) => {
    if (event.touches.length !== 2) finish();
  };
  element.addEventListener('touchstart', start, { passive: false });
  element.addEventListener('touchmove', move, { passive: false });
  element.addEventListener('touchend', end);
  element.addEventListener('touchcancel', end);
  return () => {
    releaseTargets();
    element.removeEventListener('touchstart', start);
    element.removeEventListener('touchmove', move);
    element.removeEventListener('touchend', end);
    element.removeEventListener('touchcancel', end);
  };
}

export type PaneZoom = {
  scale(): number;
  reset(): void;
  dispose(): void;
};

export function createPaneZoom(viewport: HTMLElement, surface: HTMLElement, onChange?: (scale: number) => void,
  options: { wheel?: boolean } = {}): PaneZoom {
  const maximumScale = 3;
  let scale = 1;
  let offsetX = 0;
  let offsetY = 0;
  let startScale = 1;
  let anchorX = 0;
  let anchorY = 0;
  let pinchX = 0;
  let pinchY = 0;
  let pointerId: number | null = null;
  let panTouchTarget: EventTarget | null = null;
  let panX = 0;
  let panY = 0;
  const handledTouchMoves = new WeakSet<TouchEvent>();

  const paint = () => {
    if (scale <= 1.001) {
      scale = 1;
      offsetX = 0;
      offsetY = 0;
      pointerId = null;
      surface.style.removeProperty('transform');
      surface.style.removeProperty('transform-origin');
    } else {
      surface.style.transformOrigin = '0 0';
      surface.style.transform = `translate3d(${offsetX}px, ${offsetY}px, 0) scale(${scale})`;
    }
    onChange?.(scale);
  };

  const clampOffsets = () => {
    const width = viewport.clientWidth;
    const height = viewport.clientHeight;
    offsetX = Math.min(0, Math.max(width * (1 - scale), offsetX));
    offsetY = Math.min(0, Math.max(height * (1 - scale), offsetY));
  };

  const stop = observePinch(viewport, (gesture) => {
    if (gesture.phase === 'end') return;
    const bounds = viewport.getBoundingClientRect();
    const x = gesture.clientX - bounds.left;
    const y = gesture.clientY - bounds.top;
    if (gesture.phase === 'start') {
      finishTouchPan();
      startScale = scale;
      anchorX = (x - offsetX) / scale;
      anchorY = (y - offsetY) / scale;
      pinchX = x;
      pinchY = y;
      return;
    }
    scale = Math.min(maximumScale, Math.max(1, startScale * gesture.ratio));
    // One finger pans; changing the pinch midpoint must not translate the page.
    offsetX = pinchX - anchorX * scale;
    offsetY = pinchY - anchorY * scale;
    clampOffsets();
    paint();
  });
  const translate = (x: number, y: number) => {
    offsetX += x - panX;
    offsetY += y - panY;
    panX = x;
    panY = y;
    clampOffsets();
    paint();
  };
  const pointerDown = (event: PointerEvent) => {
    if (scale === 1 || event.button !== 0 || (event.pointerType !== 'mouse' && event.pointerType !== 'pen')) return;
    pointerId = event.pointerId;
    panX = event.clientX;
    panY = event.clientY;
    viewport.setPointerCapture?.(event.pointerId);
    event.preventDefault();
    event.stopPropagation();
  };
  const pointerMove = (event: PointerEvent) => {
    if (pointerId !== event.pointerId || scale === 1) return;
    translate(event.clientX, event.clientY);
    event.preventDefault();
    event.stopPropagation();
  };
  const pointerEnd = (event: PointerEvent) => {
    if (pointerId !== event.pointerId) return;
    pointerId = null;
    if (viewport.hasPointerCapture?.(event.pointerId)) viewport.releasePointerCapture(event.pointerId);
  };
  const finishTouchPan = () => {
    if (panTouchTarget && panTouchTarget !== viewport) {
      panTouchTarget.removeEventListener('touchmove', touchMove as EventListener);
      panTouchTarget.removeEventListener('touchend', touchEnd as EventListener);
      panTouchTarget.removeEventListener('touchcancel', touchEnd as EventListener);
    }
    panTouchTarget = null;
  };
  const touchStart = (event: TouchEvent) => {
    finishTouchPan();
    if (scale === 1 || event.touches.length !== 1) return;
    const touch = event.touches[0];
    panX = touch.clientX;
    panY = touch.clientY;
    panTouchTarget = touch.target;
    if (panTouchTarget && panTouchTarget !== viewport) {
      panTouchTarget.addEventListener('touchmove', touchMove as EventListener, { passive: false });
      panTouchTarget.addEventListener('touchend', touchEnd as EventListener);
      panTouchTarget.addEventListener('touchcancel', touchEnd as EventListener);
    }
    event.preventDefault();
  };
  const touchMove = (event: TouchEvent) => {
    if (handledTouchMoves.has(event)) return;
    handledTouchMoves.add(event);
    if (!panTouchTarget) return;
    if (scale === 1 || event.touches.length !== 1) { finishTouchPan(); return; }
    translate(event.touches[0].clientX, event.touches[0].clientY);
    event.preventDefault();
  };
  const touchEnd = (event: TouchEvent) => { if (event.touches.length !== 1) finishTouchPan(); };
  viewport.addEventListener('pointerdown', pointerDown, true);
  viewport.addEventListener('pointermove', pointerMove, true);
  viewport.addEventListener('pointerup', pointerEnd, true);
  viewport.addEventListener('pointercancel', pointerEnd, true);
  viewport.addEventListener('touchstart', touchStart, { passive: false });
  viewport.addEventListener('touchmove', touchMove, { passive: false });
  viewport.addEventListener('touchend', touchEnd);
  viewport.addEventListener('touchcancel', touchEnd);
  const wheel = (event: WheelEvent) => {
    if (!event.ctrlKey) return;
    event.preventDefault();
    event.stopPropagation();
    const bounds = viewport.getBoundingClientRect();
    const x = event.clientX - bounds.left;
    const y = event.clientY - bounds.top;
    const pointX = (x - offsetX) / scale;
    const pointY = (y - offsetY) / scale;
    const pixels = event.deltaY * (event.deltaMode === WheelEvent.DOM_DELTA_LINE ? 16 :
      event.deltaMode === WheelEvent.DOM_DELTA_PAGE ? viewport.clientHeight : 1);
    scale = Math.min(maximumScale, Math.max(1, scale * Math.exp(-Math.max(-240, Math.min(240, pixels)) * 0.002)));
    offsetX = x - pointX * scale;
    offsetY = y - pointY * scale;
    clampOffsets();
    paint();
  };
  if (options.wheel) viewport.addEventListener('wheel', wheel, { capture: true, passive: false });
  const resizeObserver = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(() => {
    if (scale === 1) return;
    clampOffsets();
    paint();
  });
  resizeObserver?.observe(viewport);

  return {
    scale: () => scale,
    reset: () => {
      scale = 1;
      paint();
    },
    dispose: () => {
      stop();
      finishTouchPan();
      viewport.removeEventListener('pointerdown', pointerDown, true);
      viewport.removeEventListener('pointermove', pointerMove, true);
      viewport.removeEventListener('pointerup', pointerEnd, true);
      viewport.removeEventListener('pointercancel', pointerEnd, true);
      viewport.removeEventListener('touchstart', touchStart);
      viewport.removeEventListener('touchmove', touchMove);
      viewport.removeEventListener('touchend', touchEnd);
      viewport.removeEventListener('touchcancel', touchEnd);
      if (options.wheel) viewport.removeEventListener('wheel', wheel, true);
      resizeObserver?.disconnect();
      surface.style.removeProperty('transform');
      surface.style.removeProperty('transform-origin');
    }
  };
}
