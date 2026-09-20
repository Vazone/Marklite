export const PREVIEW_CHUNK_MIN_HTML_LENGTH = 500_000;

const NODES_PER_CHUNK = 384;
const CHUNKS_PER_GROUP = 24;

export type PreviewChunks = {
  leaves: HTMLElement[];
  groups: HTMLElement[];
  baseHeights: number[];
  measuredWidth: number;
  seenGroups: Set<HTMLElement>;
};

export function wrapPreviewChunks(fragment: DocumentFragment): PreviewChunks {
  const nodes = Array.from(fragment.childNodes);
  const leaves: HTMLElement[] = [];
  const groups: HTMLElement[] = [];
  fragment.replaceChildren();

  for (let index = 0; index < nodes.length; index += NODES_PER_CHUNK) {
    const leaf = document.createElement('div');
    leaf.className = 'preview-chunk';
    leaf.style.display = 'flow-root';
    leaf.append(...nodes.slice(index, index + NODES_PER_CHUNK));
    leaves.push(leaf);
  }
  for (let index = 0; index < leaves.length; index += CHUNKS_PER_GROUP) {
    const group = document.createElement('div');
    group.className = 'preview-chunk-group';
    group.style.display = 'flow-root';
    group.append(...leaves.slice(index, index + CHUNKS_PER_GROUP));
    fragment.append(group);
    groups.push(group);
  }
  return { leaves, groups, baseHeights: [], measuredWidth: 0, seenGroups: new Set() };
}

export function measurePreviewChunks(host: HTMLElement, chunks: PreviewChunks) {
  const leafHeights = chunks.leaves.map((leaf) => leaf.getBoundingClientRect().height);
  const groupHeights = chunks.groups.map((group) => group.getBoundingClientRect().height);
  chunks.baseHeights = leafHeights;
  chunks.measuredWidth = host.clientWidth;

  for (const [index, leaf] of chunks.leaves.entries()) {
    leaf.style.setProperty('--base-height', `${leafHeights[index]}px`);
    leaf.style.containIntrinsicSize = 'calc(var(--base-height) * var(--preview-leaf-scale, 1))';
    leaf.style.contentVisibility = 'auto';
  }
  for (const [index, group] of chunks.groups.entries()) {
    group.style.setProperty('--base-group-height', `${groupHeights[index]}px`);
    group.style.containIntrinsicSize = 'calc(var(--base-group-height) * var(--preview-group-scale, 1))';
    group.style.contentVisibility = 'auto';
  }
}

export function rescalePreviewChunks(host: HTMLElement, chunks: PreviewChunks, force = false) {
  const width = host.clientWidth;
  if (!force && Math.abs(width - chunks.measuredWidth) < 0.5) return false;

  const rect = host.getBoundingClientRect();
  const element = document.elementFromPoint?.(rect.left + rect.width / 2, rect.top + rect.height / 2);
  const visibleLeaf = element instanceof Element ? element.closest<HTMLElement>('.preview-chunk') : null;
  const visibleGroup = visibleLeaf?.parentElement;
  if (visibleGroup && host.contains(visibleGroup)) chunks.seenGroups.add(visibleGroup);
  const index = visibleLeaf && host.contains(visibleLeaf) ? chunks.leaves.indexOf(visibleLeaf) : 0;
  const leaf = chunks.leaves[Math.max(0, index)];
  const baseHeight = chunks.baseHeights[Math.max(0, index)];
  const actualHeight = leaf?.getBoundingClientRect().height ?? 0;
  const scale = baseHeight > 0 && actualHeight > 0 ? actualHeight / baseHeight : 1;

  host.style.setProperty('--preview-group-scale', String(scale));
  const offscreenGroups: HTMLElement[] = [];
  for (const group of chunks.groups) {
    group.style.setProperty('--preview-leaf-scale', String(scale));
    if (!chunks.seenGroups.has(group)) continue;
    const bounds = group.getBoundingClientRect();
    if (bounds.bottom <= rect.top || bounds.top >= rect.bottom) {
      group.style.contentVisibility = 'hidden';
      offscreenGroups.push(group);
    }
  }
  if (offscreenGroups.length) {
    // A group rendered before scrolling can retain its old intrinsic height while skipped.
    void host.scrollHeight;
    for (const group of offscreenGroups) group.style.contentVisibility = 'auto';
  }
  chunks.measuredWidth = width;
  return true;
}
