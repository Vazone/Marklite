import { describe, expect, test } from 'vitest';
import {
  adjustSidebarWidth,
  clampExpandedSidebarWidth,
  clampSidebarDragWidth,
  commitSidebarWidth,
  DEFAULT_SIDEBAR_WIDTH,
  sidebarMaximum,
  sidebarWidthFromClientX
} from './sidebarResize';

describe('sidebar resize geometry', () => {
  test('uses workspace width for the dynamic maximum', () => {
    expect(sidebarMaximum(2000)).toBe(520);
    expect(sidebarMaximum(800)).toBe(360);
    expect(sidebarMaximum(300)).toBe(200);
    expect(sidebarWidthFromClientX(480, 120)).toBe(360);
  });

  test('keeps raw collapse intent visible and commits the threshold inclusively', () => {
    expect(clampSidebarDragWidth(120, 1000)).toBe(120);
    expect(commitSidebarWidth(160, 1000)).toEqual({ mode: 'collapse', width: 160 });
    expect(commitSidebarWidth(161, 1000)).toEqual({ mode: 'expanded', width: 200 });
    expect(commitSidebarWidth(199, 1000)).toEqual({ mode: 'expanded', width: 200 });
  });

  test('clamps normal and keyboard widths without exceeding 45 percent or 520px', () => {
    expect(clampExpandedSidebarWidth(Number.NaN, 2000)).toBe(DEFAULT_SIDEBAR_WIDTH);
    expect(clampExpandedSidebarWidth(800, 1000)).toBe(450);
    expect(adjustSidebarWidth(280, -1, 1000)).toBe(264);
    expect(adjustSidebarWidth(280, 1, 1000, true)).toBe(312);
  });
});
