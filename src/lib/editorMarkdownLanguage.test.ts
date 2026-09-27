import { describe, expect, test } from 'vitest';
import { editorMarkdownLanguage } from './editorMarkdownLanguage';
import fixtures from '../shared/markdown-boundary-fixtures.json';

describe('editor Markdown language', () => {
  test.each(fixtures)('shared syntax boundary: $id', fixture => {
    const tree = editorMarkdownLanguage().language.parser.parse(fixture.source);
    for (const name of fixture.editorContains) expect(tree.toString()).toContain(name);
    const starts: number[] = [];
    const cursor = tree.cursor();
    if (cursor.firstChild()) do { starts.push(cursor.from); } while (cursor.nextSibling());
    for (const block of fixture.blocks) {
      const offset = fixture.source.indexOf(block.text);
      expect(offset).toBeGreaterThanOrEqual(0);
      expect(starts).toContain(offset);
      expect(fixture.source.slice(0, offset).split('\n')).toHaveLength(block.line);
    }
  });
  test('recognizes GFM task, table and strike syntax with product inline extensions', () => {
    const parser = editorMarkdownLanguage().language.parser;
    expect(parser.parse('- [x] done').toString()).toContain('TaskMarker');
    expect(parser.parse('| A | B |\n|---|---|\n| 1 | 2 |').toString()).toContain('TableHeader');
    expect(parser.parse('~~obsolete~~').toString()).toContain('Strikethrough');
    expect(parser.parse('H~2~O x^2^ :smile: ==mark==').toString()).toBe('Document(Paragraph(Subscript(SubscriptMark,SubscriptMark),Superscript(SuperscriptMark,SuperscriptMark),Emoji,Highlight(HighlightMark,HighlightMark)))');
  });
});
