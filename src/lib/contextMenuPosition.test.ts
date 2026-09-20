import { describe, expect, test } from 'vitest';
import { clampContextMenuPosition } from './contextMenuPosition';

describe('context menu viewport positioning', () => {
  test.each([
    [-40, -20],
    [159, -20],
    [-40, 119],
    [159, 119]
  ])('keeps all four edges inside a 160x120 viewport from (%s, %s)', (x, y) => {
    const position = clampContextMenuPosition(x, y, 220, 48, 160, 120);

    expect(position.x).toBeGreaterThanOrEqual(8);
    expect(position.y).toBeGreaterThanOrEqual(8);
    expect(position.x + position.width).toBeLessThanOrEqual(152);
    expect(position.y + position.maxHeight).toBeLessThanOrEqual(112);
    expect(position.width).toBe(144);
  });

  test('preserves the desired size and anchor when the viewport has room', () => {
    expect(clampContextMenuPosition(300, 200, 210, 116, 1024, 768)).toEqual({
      x: 300,
      y: 200,
      width: 210,
      maxHeight: 116
    });
  });
});
