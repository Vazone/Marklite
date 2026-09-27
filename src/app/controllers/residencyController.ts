import type { DocumentState, EditorTab } from '../stores/documentStore';

type Dependencies = {
  subscribe: (receive: (state: DocumentState) => void) => () => void;
  evict: (id: string) => { previewSession: string | null } | null;
  releasePreview: (id: string) => Promise<unknown>;
  onError: (error: unknown) => void;
  budget?: { bytes: number; documents: number };
};

// A logical cache budget, not a claim about total JS/native/GPU heap size.
// Active, dirty and untitled documents are protected even over this budget.
const DEFAULT_BUDGET = { bytes: 32 * 1024 * 1024, documents: 3 };

function cacheWeight(tab: EditorTab): number {
  return 2 * (tab.content.length + tab.html.length)
    + 256 * (tab.sourceBlocks.length + tab.outline.length + tab.diagrams.length);
}

export function createResidencyController(deps: Dependencies) {
  const budget = deps.budget ?? DEFAULT_BUDGET;
  const visits = new Map<string, number>();
  let clock = 0;
  let lastActive: string | null = null;
  let fingerprint = '';
  let current: DocumentState | undefined;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let unsubscribe: (() => void) | undefined;

  function trim() {
    timer = undefined;
    if (!current) return;
    const loaded = current.tabs.filter(tab => tab.loadState === 'loaded');
    let bytes = loaded.reduce((sum, tab) => sum + cacheWeight(tab), 0);
    let count = loaded.length;
    const candidates = loaded.filter(tab => tab.id !== current!.activeTabId && !tab.isDirty && tab.resource)
      .sort((left, right) => (visits.get(left.id) ?? 0) - (visits.get(right.id) ?? 0));
    for (const tab of candidates) {
      if (bytes <= budget.bytes && count <= budget.documents) break;
      const released = deps.evict(tab.id);
      if (!released) continue;
      bytes -= cacheWeight(tab);
      count--;
      if (released.previewSession) void deps.releasePreview(released.previewSession).catch(deps.onError);
    }
  }

  return {
    start() {
      if (unsubscribe) return;
      unsubscribe = deps.subscribe(state => {
        current = state;
        const ids = new Set(state.tabs.map(tab => tab.id));
        for (const id of visits.keys()) if (!ids.has(id)) visits.delete(id);
        if (state.activeTabId !== lastActive) {
          lastActive = state.activeTabId;
          if (lastActive) visits.set(lastActive, ++clock);
        }
        const next = `${state.activeTabId}|${state.tabs.map(tab =>
          `${tab.id}:${tab.loadState}:${tab.isDirty}:${tab.contentRevision}:${tab.renderedRevision}:${tab.analysisRevision}`).join('|')}`;
        if (next === fingerprint) return;
        fingerprint = next;
        // Let the outgoing editor flush its final selection/history first.
        if (timer === undefined) timer = setTimeout(trim, 0);
      });
    },
    dispose() {
      unsubscribe?.();
      unsubscribe = undefined;
      if (timer !== undefined) clearTimeout(timer);
      timer = undefined;
      current = undefined;
      visits.clear();
      fingerprint = '';
      lastActive = null;
    }
  };
}
