import { describe, expect, test } from 'vitest';
import {
  adjustSplitRatio,
  clampUnitRatio,
  clampVisibleSplitRatio,
  splitDropMode,
  splitRatioBounds,
  splitRatioFromClientX
} from './splitPane';

describe('split pane ratio policy', () => {
  test('uses inclusive 15/85 edge thresholds in the correct direction', () => {
    expect(splitDropMode(0.149)).toBe('preview');
    expect(splitDropMode(0.15)).toBe('preview');
    expect(splitDropMode(0.151)).toBe('split');
    expect(splitDropMode(0.849)).toBe('split');
    expect(splitDropMode(0.85)).toBe('edit');
    expect(splitDropMode(0.851)).toBe('edit');
  });

  test('computes pointer ratios from the editor-stage content box', () => {
    expect(splitRatioFromClientX(250, 100, 600)).toBeCloseTo(0.25);
    expect(splitRatioFromClientX(-50, 100, 600)).toBe(0);
    expect(splitRatioFromClientX(900, 100, 600)).toBe(1);
    expect(splitRatioFromClientX(100, 100, 0)).toBe(0.5);
  });

  test('clamps visible panes by pixels without changing raw drop intent', () => {
    expect(splitRatioBounds(1000)).toEqual({ min: 0.16, max: 0.84 });
    expect(clampVisibleSplitRatio(0.1, 1000)).toBe(0.16);
    expect(clampVisibleSplitRatio(0.9, 1000)).toBe(0.84);
    expect(clampVisibleSplitRatio(0.5, 300)).toBe(0.5);
  });

  test('supports 5% and Shift+10% keyboard steps within pixel bounds', () => {
    expect(adjustSplitRatio(0.5, -1, 1000)).toBeCloseTo(0.45);
    expect(adjustSplitRatio(0.5, 1, 1000, true)).toBeCloseTo(0.6);
    expect(adjustSplitRatio(0.17, -1, 1000)).toBe(0.16);
    expect(clampUnitRatio(Number.NaN)).toBe(0.5);
  });
});
