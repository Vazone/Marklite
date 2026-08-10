export const DEFAULT_SPLIT_RATIO = 0.5;
export const PREVIEW_COLLAPSE_THRESHOLD = 0.15;
export const EDIT_COLLAPSE_THRESHOLD = 0.85;
export const MIN_SPLIT_PANE_PX = 160;
export const SPLIT_KEYBOARD_STEP = 0.05;
export const SPLIT_KEYBOARD_LARGE_STEP = 0.1;

export type SplitDropMode = 'edit' | 'split' | 'preview';

export function clampUnitRatio(ratio: number): number {
  if (!Number.isFinite(ratio)) return DEFAULT_SPLIT_RATIO;
  return Math.min(1, Math.max(0, ratio));
}

export function splitDropMode(rawRatio: number): SplitDropMode {
  const ratio = clampUnitRatio(rawRatio);
  if (ratio <= PREVIEW_COLLAPSE_THRESHOLD) return 'preview';
  if (ratio >= EDIT_COLLAPSE_THRESHOLD) return 'edit';
  return 'split';
}

export function splitRatioFromClientX(clientX: number, left: number, width: number): number {
  if (!Number.isFinite(width) || width <= 0) return DEFAULT_SPLIT_RATIO;
  return clampUnitRatio((clientX - left) / width);
}

export function splitRatioBounds(width: number, minPanePx = MIN_SPLIT_PANE_PX) {
  if (!Number.isFinite(width) || width <= 0) {
    return { min: PREVIEW_COLLAPSE_THRESHOLD, max: EDIT_COLLAPSE_THRESHOLD };
  }
  const minimum = Math.min(0.5, Math.max(0, minPanePx) / width);
  return { min: minimum, max: 1 - minimum };
}

export function clampVisibleSplitRatio(
  ratio: number,
  width: number,
  minPanePx = MIN_SPLIT_PANE_PX
): number {
  const bounds = splitRatioBounds(width, minPanePx);
  return Math.min(bounds.max, Math.max(bounds.min, clampUnitRatio(ratio)));
}

export function adjustSplitRatio(
  ratio: number,
  direction: -1 | 1,
  width: number,
  largeStep = false
): number {
  const step = largeStep ? SPLIT_KEYBOARD_LARGE_STEP : SPLIT_KEYBOARD_STEP;
  return clampVisibleSplitRatio(ratio + direction * step, width);
}
