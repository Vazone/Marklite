import { describe, expect, test } from 'vitest';
import { editorMarkdownLanguage } from './editorMarkdownLanguage';

describe('editor Markdown language', () => {
  test('recognizes GFM task, table and strike syntax without unsupported shorthand', () => {
    const parser = editorMarkdownLanguage().language.parser;
    expect(parser.parse('- [x] done').toString()).toContain('TaskMarker');
    expect(parser.parse('| A | B |\n|---|---|\n| 1 | 2 |').toString()).toContain('TableHeader');
    expect(parser.parse('~~obsolete~~').toString()).toContain('Strikethrough');
    expect(parser.parse('H~2~O x^2^ :smile: ==mark==').toString()).toBe('Document(Paragraph)');
  });
});
