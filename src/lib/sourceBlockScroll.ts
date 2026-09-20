import type { SourceBlock } from './tauriApi';

export type SourceBlockTarget = {
  element: HTMLElement;
  fraction: number;
};

export type SourceBlockScrollMap = {
  sourceBlocks: readonly SourceBlock[];
  elements: readonly HTMLElement[];
};

const BLOCK_MARKER = /\ue000MARKLITE_BLOCK_(\d+)\ue001/g;
export const SOURCE_BLOCK_ATTRIBUTE = 'data-marklite-source-block';

export function tagSourceBlockElements(fragment: DocumentFragment | HTMLElement, baseIndex = 0): void {
  let pending: string | null = null;
  for (const node of Array.from(fragment.childNodes)) {
    if (node.nodeType === Node.TEXT_NODE) {
      const value = node.textContent ?? '';
      BLOCK_MARKER.lastIndex = 0;
      for (const match of value.matchAll(BLOCK_MARKER)) pending = match[1];
      const withoutMarkers = value.replace(BLOCK_MARKER, '');
      if (withoutMarkers) node.textContent = withoutMarkers;
      else node.parentNode?.removeChild(node);
    } else if (node instanceof HTMLElement && pending !== null) {
      node.setAttribute(SOURCE_BLOCK_ATTRIBUTE, String(Number(pending) - baseIndex));
      pending = null;
    }
  }
}

export function buildSourceBlockScrollMap(
  sourceBlocks: readonly SourceBlock[],
  elements: readonly HTMLElement[]
): SourceBlockScrollMap | null {
  if (!sourceBlocks.length || sourceBlocks.length !== elements.length) return null;
  if (elements.some((element, index) => element.getAttribute(SOURCE_BLOCK_ATTRIBUTE) !== String(index))) return null;
  return { sourceBlocks, elements };
}

export function findSourceBlockTarget(map: SourceBlockScrollMap, offsetUtf16: number): SourceBlockTarget {
  const { sourceBlocks, elements } = map;
  let left = 0;
  let right = sourceBlocks.length;
  while (left < right) {
    const middle = (left + right) >>> 1;
    if (sourceBlocks[middle].endUtf16 <= offsetUtf16) left = middle + 1;
    else right = middle;
  }
  const index = Math.min(left, sourceBlocks.length - 1);
  const block = sourceBlocks[index];
  const span = block.endUtf16 - block.startUtf16;
  const fraction = span > 0 ? Math.min(1, Math.max(0, (offsetUtf16 - block.startUtf16) / span)) : 0;
  return { element: elements[index], fraction };
}

export function findVisibleSourceBlockLine(map: SourceBlockScrollMap, host: HTMLElement): number | null {
  const bounds = host.getBoundingClientRect();
  if (bounds.width <= 0 || bounds.height <= 0 || !host.ownerDocument.elementFromPoint) return null;
  // Hit testing visits painted content without forcing layout of offscreen content-visibility groups.
  // Probe past block margins, while preferring the source block nearest the viewport top.
  for (const offset of [2, 16, 32, 64, 128, bounds.height / 2]) {
    const y = bounds.top + Math.min(offset, bounds.height - 1);
    for (const fraction of [0.5, 0.25, 0.75]) {
      const hit = host.ownerDocument.elementFromPoint(bounds.left + bounds.width * fraction, y);
      const element = hit?.closest<HTMLElement>(`[${SOURCE_BLOCK_ATTRIBUTE}]`);
      if (!element || !host.contains(element)) continue;
      const index = Number(element.getAttribute(SOURCE_BLOCK_ATTRIBUTE));
      if (Number.isSafeInteger(index) && index >= 0 && map.elements[index] === element) {
        return map.sourceBlocks[index].startLine;
      }
    }
  }
  return null;
}
