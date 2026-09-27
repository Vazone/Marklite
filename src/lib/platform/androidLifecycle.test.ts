import { describe, expect, test } from 'vitest';
import { androidAvailableViewportHeight, parseAndroidSystemInsets } from './androidLifecycle';

describe('Android system insets', () => {
  test('accepts measured CSS pixel distances on all four sides', () => {
    expect(parseAndroidSystemInsets({ top: 52, right: 0, bottom: 32, left: 0, imeBottom: 340 }))
      .toEqual({ top: 52, right: 0, bottom: 32, left: 0, imeBottom: 340 });
  });

  test('rejects missing or invalid native measurements', () => {
    for (const value of [null, { top: 52, right: 0, bottom: 32, left: 0 },
      { top: 52, right: 0, bottom: -1, left: 0, imeBottom: 0 },
      { top: 52, right: 0, bottom: Infinity, left: 0, imeBottom: 0 },
      { top: 52, right: 0, bottom: 32, left: 0, imeBottom: -1 }]) {
      expect(() => parseAndroidSystemInsets(value)).toThrow();
    }
  });

  test('fits the IME on older WebViews without double shrinking newer visual viewports', () => {
    expect(androidAvailableViewportHeight(915, 915, 336)).toBe(579);
    expect(androidAvailableViewportHeight(915, 579, 336)).toBe(579);
    expect(androidAvailableViewportHeight(915, 915, 0)).toBe(915);
  });
});
