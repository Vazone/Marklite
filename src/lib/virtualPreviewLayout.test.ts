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
