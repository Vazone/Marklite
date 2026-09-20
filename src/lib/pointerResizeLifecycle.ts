export type PointerResizeLifecycle = {
  readonly active: boolean;
  readonly pointerId: number | null;
  start(event: Pick<PointerEvent, 'pointerId'>): boolean;
  schedule(clientX: number): void;
  commit(event: Pick<PointerEvent, 'pointerId'>): boolean;
  cancel(event: Pick<PointerEvent, 'pointerId'>): boolean;
  cancelActive(): boolean;
  handleLostPointerCapture(event: Pick<PointerEvent, 'pointerId'>): boolean;
  dispose(): void;
};

type PointerResizeLifecycleOptions = {
  element: HTMLElement;
  bodyClass?: string;
  onSample?: (clientX: number) => void;
  onStart?: () => void;
  onCommit?: () => void;
  onCancel?: () => void;
};

export function createPointerResizeLifecycle(
  options: PointerResizeLifecycleOptions
): PointerResizeLifecycle {
  let activePointerId: number | null = null;
  let frame: number | null = null;
  let pendingClientX: number | null = null;

  function cancelFrame(): void {
    if (frame !== null) cancelAnimationFrame(frame);
    frame = null;
    pendingClientX = null;
  }

  function clearOwnership(): number | null {
    const capturedPointerId = activePointerId;
    activePointerId = null;
    if (options.bodyClass) document.body.classList.remove(options.bodyClass);
    return capturedPointerId;
  }

  function release(pointerId: number): void {
    if (options.element.hasPointerCapture?.(pointerId)) {
      options.element.releasePointerCapture(pointerId);
    }
  }

  function cancelOwned(pointerId: number): boolean {
    if (activePointerId !== pointerId) return false;
    cancelFrame();
    const capturedPointerId = clearOwnership();
    options.onCancel?.();
    if (capturedPointerId !== null) release(capturedPointerId);
    return true;
  }

  return {
    get active() {
      return activePointerId !== null;
    },
    get pointerId() {
      return activePointerId;
    },
    start(event) {
      if (activePointerId !== null) return false;
      activePointerId = event.pointerId;
      options.onStart?.();
      options.element.setPointerCapture?.(event.pointerId);
      if (options.bodyClass) document.body.classList.add(options.bodyClass);
      return true;
    },
    schedule(clientX) {
      if (activePointerId === null) return;
      pendingClientX = clientX;
      if (frame !== null) return;
      frame = requestAnimationFrame(() => {
        frame = null;
        const nextClientX = pendingClientX;
        pendingClientX = null;
        if (activePointerId !== null && nextClientX !== null) options.onSample?.(nextClientX);
      });
    },
    commit(event) {
      if (activePointerId !== event.pointerId) return false;
      cancelFrame();
      const capturedPointerId = clearOwnership();
      options.onCommit?.();
      if (capturedPointerId !== null) release(capturedPointerId);
      return true;
    },
    cancel(event) {
      return cancelOwned(event.pointerId);
    },
    cancelActive() {
      return activePointerId === null ? false : cancelOwned(activePointerId);
    },
    handleLostPointerCapture(event) {
      if (activePointerId !== event.pointerId) return false;
      cancelFrame();
      clearOwnership();
      options.onCancel?.();
      return true;
    },
    dispose() {
      if (!this.cancelActive()) cancelFrame();
    }
  };
}
