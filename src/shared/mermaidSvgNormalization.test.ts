import { describe, expect, test, vi } from 'vitest';
import { normalizeMermaidSvgDocument, parseMermaidSvgDocument } from './mermaidSvgNormalization.mjs';

describe('Mermaid SVG XML boundary', () => {
  test('closes the HTML break emitted for a flowchart label', () => {
    const markup = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 50"><foreignObject><div xmlns="http://www.w3.org/1999/xhtml"><p>Is Mobile &amp;<br>Use Local DB?</p></div></foreignObject></svg>';
    const root = parseMermaidSvgDocument(markup);
    expect(root.localName).toBe('svg');
    expect(root.querySelector('br')?.namespaceURI).toBe('http://www.w3.org/1999/xhtml');
    expect(new XMLSerializer().serializeToString(root)).toContain('<br />');
    vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockImplementation(() => null);
    normalizeMermaidSvgDocument(root);
    expect([...root.querySelectorAll('tspan')].map((line) => line.textContent)).toEqual([
      'Is Mobile &', 'Use Local DB?'
    ]);
  });

  test('still rejects other malformed markup', () => {
    expect(() => parseMermaidSvgDocument('<svg><script></svg>')).toThrow('invalid SVG XML');
  });
});
