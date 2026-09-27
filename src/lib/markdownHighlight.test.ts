import { expect, test } from 'vitest';
import { TreeFragment, type Tree } from '@lezer/common';
import { highlightTree } from '@lezer/highlight';
import { editorHighlightStyle, editorMarkdownLanguage } from './editorMarkdownLanguage';
import { markdownParser } from './markdownParser';
import fixtures from '../shared/markdown-highlight-fixtures.json';
import { highlightTag } from './markdownMarks';

function nodes(tree: Tree) {
  const result: Array<[string, number, number]> = [];
  tree.iterate({ enter: node => { result.push([node.name, node.from, node.to]); } });
  return result;
}

test.each(fixtures)('shared highlight boundary: $id', ({ source, marks }) => {
  const found = nodes(editorMarkdownLanguage().language.parser.parse(source)).filter(([name]) => name === 'Highlight');
  expect(found).toHaveLength(marks);
  for (const [, from, to] of found) {
    expect(source.slice(from, from + 2)).toBe('==');
    expect(source.slice(to - 2, to)).toBe('==');
  }
});

test('highlight preserves nested formatting and existing syntax styling', () => {
  const source = '==**bold** [link](https://example.com)==';
  const tree = editorMarkdownLanguage().language.parser.parse(source);
  const highlight = tree.topNode.firstChild!.getChild('Highlight')!;
  expect(highlight.getChild('StrongEmphasis')).not.toBeNull();
  expect(highlight.getChild('Link')).not.toBeNull();
  const styled: string[] = [];
  highlightTree(tree, editorHighlightStyle, (from, to, classes) => {
    const text = source.slice(from, to);
    styled.push(text);
    if (text === 'bold' || text === 'link') {
      expect(classes.split(' ')).toContain(editorHighlightStyle.style([highlightTag]));
    }
  });
  expect(styled.join('')).toContain('bold');
  expect(styled.join('')).toContain('==');
});

test('incremental edits match a fresh parse and never duplicate highlight nodes', () => {
  let source = 'first\n\n==**bold**== :smile:\n\nlast';
  let tree = markdownParser.parse(source);
  for (const { from, to, insert } of [
    { from: 0, to: 5, insert: '中文😀' },
    { from: 8, to: 10, insert: '=' },
    { from: 8, to: 9, insert: '==' },
    { from: 0, to: 0, insert: '==new==\n\n' }
  ]) {
    const next = source.slice(0, from) + insert + source.slice(to);
    const fragments = TreeFragment.applyChanges(TreeFragment.addTree(tree), [
      { fromA: from, toA: to, fromB: from, toB: from + insert.length }
    ]);
    tree = markdownParser.parse(next, fragments);
    expect(nodes(tree)).toEqual(nodes(markdownParser.parse(next)));
    source = next;
  }
});

test('partial range uses absolute input positions with relative output nodes', () => {
  const source = 'prefix\n\n==tail==';
  const tree = markdownParser.parse(source, [], [{ from: 8, to: source.length }]);
  expect(nodes(tree).filter(([name]) => name === 'Highlight')).toEqual([['Highlight', 0, 8]]);
});

test('a 500 KiB edit reuses processed blocks instead of rereading the full document', () => {
  const seed = 'text ==highlight== **bold** H~2~O x^2^ :smile:\n\n';
  const source = seed.repeat(Math.ceil(512000 / seed.length)).slice(0, 512000);
  const tree = markdownParser.parse(source);
  const next = 'X' + source.slice(1);
  let readBytes = 0;
  const input = {
    length: next.length, lineChunks: false,
    chunk: (from: number) => next.slice(from),
    read: (from: number, to: number) => { readBytes += to - from; return next.slice(from, to); }
  };
  const fragments = TreeFragment.applyChanges(TreeFragment.addTree(tree), [{ fromA: 0, toA: 1, fromB: 0, toB: 1 }]);
  const updated = markdownParser.parse(input, fragments);
  expect(updated.length).toBe(512000);
  expect(readBytes).toBeLessThan(32768);
  expect(nodes(updated)).toEqual(nodes(markdownParser.parse(next)));
});
