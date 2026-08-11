export const DEFAULT_SIDEBAR_WIDTH = 280;
export const MIN_SIDEBAR_WIDTH = 200;
export const MAX_SIDEBAR_WIDTH = 520;
export const SIDEBAR_MAX_WORKSPACE_RATIO = 0.45;
export const SIDEBAR_COLLAPSE_THRESHOLD = 160;
export const SIDEBAR_DRAG_VISIBLE_MIN = 96;
export const SIDEBAR_KEYBOARD_STEP = 16;
export const SIDEBAR_KEYBOARD_LARGE_STEP = 32;

export type SidebarDropResult =
  | { mode: 'collapse'; width: number }
  | { mode: 'expanded'; width: number };

export function sidebarMaximum(workspaceWidth: number): number {
  if (!Number.isFinite(workspaceWidth) || workspaceWidth <= 0) return MAX_SIDEBAR_WIDTH;
  return Math.max(
    MIN_SIDEBAR_WIDTH,
    Math.min(MAX_SIDEBAR_WIDTH, workspaceWidth * SIDEBAR_MAX_WORKSPACE_RATIO)
  );
}

export function sidebarWidthFromClientX(clientX: number, workspaceLeft: number): number {
  if (!Number.isFinite(clientX) || !Number.isFinite(workspaceLeft)) {
    return DEFAULT_SIDEBAR_WIDTH;
  }
  return clientX - workspaceLeft;
}

export function clampSidebarDragWidth(rawWidth: number, workspaceWidth: number): number {
  const fallback = Number.isFinite(rawWidth) ? rawWidth : DEFAULT_SIDEBAR_WIDTH;
  return Math.min(sidebarMaximum(workspaceWidth), Math.max(SIDEBAR_DRAG_VISIBLE_MIN, fallback));
}

export function clampExpandedSidebarWidth(width: number, workspaceWidth: number): number {
  const fallback = Number.isFinite(width) ? width : DEFAULT_SIDEBAR_WIDTH;
  return Math.min(sidebarMaximum(workspaceWidth), Math.max(MIN_SIDEBAR_WIDTH, fallback));
}

export function commitSidebarWidth(rawWidth: number, workspaceWidth: number): SidebarDropResult {
  if (Number.isFinite(rawWidth) && rawWidth <= SIDEBAR_COLLAPSE_THRESHOLD) {
    return { mode: 'collapse', width: SIDEBAR_COLLAPSE_THRESHOLD };
  }
  return {
    mode: 'expanded',
    width: clampExpandedSidebarWidth(rawWidth, workspaceWidth)
  };
}

export function adjustSidebarWidth(
  width: number,
  direction: -1 | 1,
  workspaceWidth: number,
  largeStep = false
): number {
  const step = largeStep ? SIDEBAR_KEYBOARD_LARGE_STEP : SIDEBAR_KEYBOARD_STEP;
  return clampExpandedSidebarWidth(width + direction * step, workspaceWidth);
}
