import { describe, expect, test } from 'vitest';
import type { OutlineItem } from './tauriApi';
import { outlineItemAtLine } from './outlineSelection';

function heading(line: number, title = `Heading ${line}`): OutlineItem {
  return { level: 1, title, line, slug: title.toLowerCase().replaceAll(' ', '-') };
}

describe('outline item selection', () => {
  test('selects the last heading at or before the current line', () => {
    const outline = [heading(2), heading(10, 'First at ten'), heading(10, 'Second at ten'), heading(30)];

    expect(outlineItemAtLine(outline, 1)).toBeUndefined();
    expect(outlineItemAtLine(outline, 2)).toBe(outline[0]);
    expect(outlineItemAtLine(outline, 10)).toBe(outline[2]);
    expect(outlineItemAtLine(outline, 29)).toBe(outline[2]);
    expect(outlineItemAtLine(outline, 300)).toBe(outline[3]);
  });

  test('reads logarithmically from a large ordered outline', () => {
    const source = Array.from({ length: 10_000 }, (_, index) => heading(index * 3 + 1));
    let indexedReads = 0;
    const outline = new Proxy(source, {
      get(target, property, receiver) {
        if (typeof property === 'string' && /^\d+$/.test(property)) indexedReads += 1;
        return Reflect.get(target, property, receiver);
      }
    });

    expect(outlineItemAtLine(outline, 20_000)?.line).toBe(19_999);
    expect(indexedReads).toBeLessThan(32);
  });
});
