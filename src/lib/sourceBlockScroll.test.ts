import { describe, expect, test, vi } from 'vitest';
import type { SourceBlock } from './tauriApi';
import { buildSourceBlockScrollMap, findSourceBlockTarget, findVisibleSourceBlockLine, tagSourceBlockElements } from './sourceBlockScroll';

describe('source block scroll mapping', () => {
  test('finds a painted nested block without measuring offscreen blocks and rejects stale identities', () => {
    const host = document.createElement('div');
    host.innerHTML = '<div class="preview-chunk"><p data-marklite-source-block="0">Offscreen</p><p data-marklite-source-block="1"><strong>Visible</strong></p></div>';
    const elements = [...host.querySelectorAll<HTMLElement>('p')];
    const map = buildSourceBlockScrollMap([
      { startUtf16: 0, endUtf16: 9, startLine: 1, endLine: 1 },
      { startUtf16: 10, endUtf16: 20, startLine: 3, endLine: 3 }
    ], elements)!;
    const measures = elements.map((element) => vi.spyOn(element, 'getBoundingClientRect'));
    vi.spyOn(host, 'getBoundingClientRect').mockReturnValue({ top: 0, left: 0, width: 500, height: 500 } as DOMRect);
    const original = Object.getOwnPropertyDescriptor(document, 'elementFromPoint');
    const hit = vi.fn((_x: number, y: number) => y < 16 ? host : host.querySelector('strong'));
    Object.defineProperty(document, 'elementFromPoint', { configurable: true, value: hit });
    try {
      expect(findVisibleSourceBlockLine(map, host)).toBe(3);
      expect(hit).toHaveBeenCalledTimes(4);
      expect(measures.every((measure) => measure.mock.calls.length === 0)).toBe(true);
      elements[1].replaceWith(elements[1].cloneNode(true));
      expect(findVisibleSourceBlockLine(map, host)).toBeNull();
    } finally {
      if (original) Object.defineProperty(document, 'elementFromPoint', original);
      else Reflect.deleteProperty(document, 'elementFromPoint');
      vi.restoreAllMocks();
    }
  });

  test('uses source order, moves blank-line gaps to the next block, and locates progress in a tall block', () => {
    const sourceBlocks: SourceBlock[] = [
      { startUtf16: 0, endUtf16: 8, startLine: 1, endLine: 1 },
      { startUtf16: 10, endUtf16: 30, startLine: 3, endLine: 8 },
      { startUtf16: 32, endUtf16: 42, startLine: 10, endLine: 10 }
    ];
    const elements = sourceBlocks.map((_, index) => {
      const element = document.createElement('p');
      element.dataset.markliteSourceBlock = String(index);
      return element;
    });
    const map = buildSourceBlockScrollMap(sourceBlocks, elements);
    expect(map).not.toBeNull();
    expect(buildSourceBlockScrollMap(sourceBlocks, elements.slice(1))).toBeNull();
    expect(findSourceBlockTarget(map!, 5)).toEqual({ element: elements[0], fraction: 0.625 });
    expect(findSourceBlockTarget(map!, 9)).toEqual({ element: elements[1], fraction: 0 });
    expect(findSourceBlockTarget(map!, 20)).toEqual({ element: elements[1], fraction: 0.5 });
    expect(findSourceBlockTarget(map!, 99)).toEqual({ element: elements[2], fraction: 1 });
  });

  test('removes transport markers and maps a block that renders multiple sibling elements', () => {
    const template = document.createElement('template');
    template.innerHTML = '\ue000MARKLITE_BLOCK_0\ue001<p>Before</p>\ue000MARKLITE_BLOCK_1\ue001<div class="math-error">Formula limit</div><p></p>\ue000MARKLITE_BLOCK_2\ue001<p>After</p>';
    tagSourceBlockElements(template.content);
    expect(template.textContent).not.toContain('MARKLITE_BLOCK');
    const elements = [...template.content.querySelectorAll<HTMLElement>('[data-marklite-source-block]')];
    expect(elements.map((element) => element.tagName)).toEqual(['P', 'DIV', 'P']);
    expect(template.content.children).toHaveLength(4);
    const blocks: SourceBlock[] = [
      { startUtf16: 0, endUtf16: 6, startLine: 1, endLine: 1 },
      { startUtf16: 8, endUtf16: 20, startLine: 3, endLine: 3 },
      { startUtf16: 22, endUtf16: 27, startLine: 5, endLine: 5 }
    ];
    const map = buildSourceBlockScrollMap(blocks, elements);
    expect(map).not.toBeNull();
    expect(findSourceBlockTarget(map!, 23).element.textContent).toBe('After');
    expect(buildSourceBlockScrollMap(blocks, [...elements].reverse())).toBeNull();
  });
});
