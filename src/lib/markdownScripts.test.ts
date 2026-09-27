import { expect, test } from 'vitest';
import { markdownParser } from './markdownParser';
import fixtures from '../shared/markdown-script-fixtures.json';
import { TreeFragment } from '@lezer/common';

test.each(fixtures)('shared script boundary: $id', ({ source, sub, sup }) => {
  let subs = 0, supers = 0;
  markdownParser.parse(source).iterate({ enter: node => {
    if (node.name === 'Subscript') subs++;
    if (node.name === 'Superscript') supers++;
    if (node.name === 'Subscript' || node.name === 'Superscript') {
      const marker = node.name === 'Subscript' ? '~' : '^';
      expect(source[node.from]).toBe(marker);
      expect(source[node.to - 1]).toBe(marker);
    }
  } });
  expect([subs, supers]).toEqual([sub, sup]);
});

test('incremental script edits preserve source positions and match fresh parsing', () => {
  let source = '中文😀 H~2~O\n\n==x^2^==\n\nend';
  let tree = markdownParser.parse(source);
  for (const { from, to, insert } of [
    { from: 7, to: 7, insert: ' ' },
    { from: 7, to: 7, insert: '\\' },
    { from: 0, to: 0, insert: '~a^b~c^\n\n' },
    { from: 0, to: 1, insert: '' }
  ]) {
    const next = source.slice(0, from) + insert + source.slice(to);
    const fragments = TreeFragment.applyChanges(TreeFragment.addTree(tree), [
      { fromA: from, toA: to, fromB: from, toB: from + insert.length }
    ]);
    tree = markdownParser.parse(next, fragments);
    const positions = (parsed: typeof tree) => {
      const result: unknown[] = [];
      parsed.iterate({ enter: node => { result.push([node.name, node.from, node.to]); } });
      return result;
    };
    expect(positions(tree)).toEqual(positions(markdownParser.parse(next)));
    source = next;
  }
});

test('math protects script and highlight markers', () => {
  const tree = markdownParser.parse('$x^2^+H~2~O+==a==$');
  expect(tree.toString()).toContain('MathSource');
  expect(tree.toString()).not.toMatch(/Subscript|Superscript|Highlight/);
});
