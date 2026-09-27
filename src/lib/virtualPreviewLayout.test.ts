import { describe, expect, it } from 'vitest';
import type { VirtualPreviewSegment } from './tauriApi';
import { previewHeightPrefix, previewSegmentAtLine, previewSegmentAtPixel, previewSegmentAtSource, previewWindowAround } from './virtualPreviewLayout';

function segments(count: number): VirtualPreviewSegment[] {
  return Array.from({ length: count }, (_, index) => ({
    startUtf16: index * 100,
    endUtf16: (index + 1) * 100,
    startLine: index * 10 + 1,
    endLine: (index + 1) * 10,
    estimatedHeight: 300,
    estimatedNodes: 120
  }));
}

describe('virtual preview layout', () => {
  it('locates source and pixels at boundaries without scanning the document DOM', () => {
    const indexed = segments(1000);
    const prefix = previewHeightPrefix(indexed.map((segment) => segment.estimatedHeight));
    expect(previewSegmentAtPixel(prefix, 0)).toBe(0);
    expect(previewSegmentAtPixel(prefix, 300)).toBe(1);
    expect(previewSegmentAtPixel(prefix, prefix[prefix.length - 1])).toBe(999);
    expect(previewSegmentAtSource(indexed, 250)).toBe(2);
    expect(previewSegmentAtSource(indexed, 100_000)).toBe(999);
    expect(previewSegmentAtLine(indexed, 901)).toBe(90);
  });

  it('maps a shared TOC source to its first piece while pixel scrolling reaches every continuation', () => {
    const indexed: VirtualPreviewSegment[] = Array.from({ length: 80 }, (_, index) => ({
      startUtf16: 0, endUtf16: 6, startLine: 1, endLine: 2,
      estimatedHeight: 3000, estimatedNodes: 385, sourceContinuation: index > 0
    }));
    indexed.push({ startUtf16: 8, endUtf16: 20, startLine: 3, endLine: 4, estimatedHeight: 50, estimatedNodes: 2 });
    const prefix = previewHeightPrefix(indexed.map(item => item.estimatedHeight));
    expect(previewSegmentAtSource(indexed, 0)).toBe(0);
    expect(previewSegmentAtSource(indexed, 5)).toBe(0);
    expect(previewSegmentAtSource(indexed, 8)).toBe(80);
    expect(previewSegmentAtLine(indexed, 1)).toBe(0);
    for (let index = 0; index < 80; index++) {
      expect(previewSegmentAtPixel(prefix, prefix[index] + 1)).toBe(index);
      const window = previewWindowAround(indexed, prefix, index, 800);
      expect(window.start).toBeLessThanOrEqual(index);
      expect(window.end).toBeGreaterThan(index);
      expect(indexed.slice(window.start, window.end).reduce((sum, item) => sum + item.estimatedNodes, 0)).toBeLessThanOrEqual(8000);
    }
  });

  it('keeps the requested segment and bounds its neighbors', () => {
    const indexed = segments(1000);
    const prefix = previewHeightPrefix(indexed.map((segment) => segment.estimatedHeight));
    const range = previewWindowAround(indexed, prefix, 900, 800);
    expect(range.start).toBeLessThanOrEqual(900);
    expect(range.end).toBeGreaterThan(900);
    expect(range.end - range.start).toBeLessThanOrEqual(8);
    expect(range.start).toBeGreaterThan(890);
  });
});
