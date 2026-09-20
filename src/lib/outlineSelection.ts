import type { OutlineItem } from './tauriApi';

/** Returns the last heading whose source line is not after the current line. */
export function outlineItemAtLine(
  outline: readonly OutlineItem[],
  currentLine: number
): OutlineItem | undefined {
  let lower = 0;
  let upper = outline.length;

  while (lower < upper) {
    const middle = lower + Math.floor((upper - lower) / 2);
    if (outline[middle].line <= currentLine) lower = middle + 1;
    else upper = middle;
  }

  return lower === 0 ? undefined : outline[lower - 1];
}
