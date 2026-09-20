import type { EditorScrollPosition } from '../app/stores/documentStore';

export type SplitScrollAnchor = {
  tabId: string;
  contentRevision: number;
  position: EditorScrollPosition;
};

export function createSplitScrollController(apply: (anchor: SplitScrollAnchor) => void) {
  let activeTabId: string | null = null;
  let activeRevision = -1;
  let latest: SplitScrollAnchor | null = null;
  let frame = 0;
  let layoutSettled = true;
  let disposed = false;

  function schedule() {
    if (disposed || !layoutSettled || frame || !latest || latest.tabId !== activeTabId || latest.contentRevision !== activeRevision) return;
    frame = requestAnimationFrame(() => {
      frame = 0;
      if (!disposed && layoutSettled && latest?.tabId === activeTabId && latest.contentRevision === activeRevision) apply(latest);
    });
  }

  return {
    setActive(tabId: string, contentRevision: number) {
      if (activeTabId !== tabId || activeRevision !== contentRevision) {
        cancelAnimationFrame(frame);
        frame = 0;
        if (latest && (latest.tabId !== tabId || latest.contentRevision !== contentRevision)) latest = null;
        activeTabId = tabId;
        activeRevision = contentRevision;
      }
      schedule();
    },
    update(anchor: SplitScrollAnchor) {
      latest = anchor;
      schedule();
    },
    layoutPending() {
      layoutSettled = false;
      cancelAnimationFrame(frame);
      frame = 0;
    },
    layoutReady() {
      layoutSettled = true;
      schedule();
    },
    clear() {
      cancelAnimationFrame(frame);
      frame = 0;
      latest = null;
    },
    dispose() {
      disposed = true;
      cancelAnimationFrame(frame);
      frame = 0;
      latest = null;
    }
  };
}
