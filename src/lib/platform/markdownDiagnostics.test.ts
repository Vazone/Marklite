import { describe, expect, it } from 'vitest';
import fixtures from '../../shared/desktop-contract-fixtures.json';
import { parseMarkdownAnalysisDto, parseRenderedMarkdownDto } from './contractValidation';

const diagnostic = { code: 'FRONT_MATTER_INVALID', message: 'Invalid YAML', line: 2, column: 3 };

describe('Markdown diagnostic contracts', () => {
  for (const [parse, fixture] of [
    [parseMarkdownAnalysisDto, fixtures.markdownAnalysis],
    [parseRenderedMarkdownDto, fixtures.renderedMarkdown]
  ] as const) {
    it(`${parse.name} preserves source locations and accepts older payloads`, () => {
      expect(parse(fixture).markdownDiagnostics).toBeUndefined();
      expect(parse({ ...fixture, markdownDiagnostics: [diagnostic] }).markdownDiagnostics).toEqual([diagnostic]);
    });
    it(`${parse.name} rejects invalid diagnostic locations and shapes`, () => {
      for (const value of [null, {}, [{ ...diagnostic, line: 0 }], [{ ...diagnostic, column: -1 }],
        [{ ...diagnostic, line: 1.5 }], [{ ...diagnostic, message: null }]]) {
        expect(() => parse({ ...fixture, markdownDiagnostics: value })).toThrow();
      }
    });
  }
});


describe('virtual source continuations', () => {
  const first = { startUtf16: 0, endUtf16: 6, startLine: 1, endLine: 2, estimatedHeight: 100, estimatedNodes: 300 };
  const parse = (segments: unknown[]) => parseRenderedMarkdownDto({
    ...fixtures.renderedMarkdown, virtualPreview: { sessionId: 'toc', segments }
  });
  it('accepts explicitly declared repeated source ranges', () => {
    const result = parse([first, { ...first, sourceContinuation: true },
      { ...first, startUtf16: 8, endUtf16: 18, startLine: 3, endLine: 4 }]);
    expect(result.virtualPreview?.segments).toHaveLength(3);
  });
  it('rejects missing, mismatched or malformed continuation declarations', () => {
    for (const segments of [
      [first, first],
      [{ ...first, sourceContinuation: true }],
      [first, { ...first, endUtf16: 7, sourceContinuation: true }],
      [first, { ...first, startLine: 2, sourceContinuation: true }],
      [first, { ...first, sourceContinuation: 'true' }]
    ]) expect(() => parse(segments)).toThrow();
  });
});
