import type { VirtualPreviewSegment } from './tauriApi';

const MAX_WINDOW_SEGMENTS = 8;
const MAX_WINDOW_NODES = 8_000;

export function previewHeightPrefix(heights: readonly number[]): number[] {
  const prefix = [0];
  for (const height of heights) prefix.push(prefix[prefix.length - 1] + Math.max(1, height));
  return prefix;
}

export function previewSegmentAtPixel(prefix: readonly number[], pixel: number): number {
  if (prefix.length <= 1) return 0;
  let low = 0;
  let high = prefix.length - 1;
  while (low + 1 < high) {
    const middle = (low + high) >>> 1;
    if (prefix[middle] <= pixel) low = middle;
    else high = middle;
  }
  return Math.min(low, prefix.length - 2);
}

export function previewSegmentAtSource(
  segments: readonly VirtualPreviewSegment[],
  offsetUtf16: number
): number {
  if (!segments.length) return 0;
  let low = 0;
  let high = segments.length;
  while (low < high) {
    const middle = (low + high) >>> 1;
    if (segments[middle].endUtf16 <= offsetUtf16) low = middle + 1;
    else high = middle;
  }
  return Math.min(low, segments.length - 1);
}

export function previewSegmentAtLine(
  segments: readonly VirtualPreviewSegment[],
  line: number
): number {
  if (!segments.length) return 0;
  let low = 0;
  let high = segments.length;
  while (low < high) {
    const middle = (low + high) >>> 1;
    if (segments[middle].endLine < line) low = middle + 1;
    else high = middle;
  }
  return Math.min(low, segments.length - 1);
}

export function previewWindowAround(
  segments: readonly VirtualPreviewSegment[],
  prefix: readonly number[],
  index: number,
  viewportHeight: number
): { start: number; end: number } {
  if (!segments.length) return { start: 0, end: 0 };
  const target = Math.max(0, Math.min(segments.length - 1, index));
  const overscan = Math.max(1, viewportHeight) * 2;
  let start = target;
  let end = target + 1;
  let nodes = segments[target].estimatedNodes;
  while (end - start < MAX_WINDOW_SEGMENTS) {
    const canGrowBefore = start > 0 && prefix[target] - prefix[start] < overscan;
    const canGrowAfter = end < segments.length && prefix[end] - prefix[target] < overscan * 2;
    if (!canGrowBefore && !canGrowAfter) break;
    const growBefore = canGrowBefore && (!canGrowAfter || prefix[target] - prefix[start] <= prefix[end] - prefix[target]);
    const candidate = growBefore ? start - 1 : end;
    const candidateNodes = segments[candidate].estimatedNodes;
    if (nodes + candidateNodes > MAX_WINDOW_NODES) break;
    nodes += candidateNodes;
    if (growBefore) start--;
    else end++;
  }
  return { start, end };
}
