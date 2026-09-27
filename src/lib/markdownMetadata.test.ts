import { expect, test } from 'vitest';
import { TreeFragment } from '@lezer/common';
import { markdownParser } from './markdownParser';

test.each(['---\ntitle: Example\n---\n', '\uFEFF---\r\ntitle: 中文\r\n...\r\n', '---\n---\n'])('metadata source envelope: %s', header => {
  const source = header + '# Body\n\n==marked== :smile:';
  const tree = markdownParser.parse(source);
  expect(tree.topNode.firstChild?.name).toBe('FrontMatter');
  expect(tree.topNode.firstChild?.to).toBe(header.length);
  expect(tree.topNode.getChild('ATXHeading1')?.from).toBe(header.length);
  expect(tree.toString()).toContain('Highlight');
  expect(tree.toString()).toContain('Emoji');
});

test('metadata values are opaque to Markdown extensions', () => {
  const tree = markdownParser.parse('---\ntitle: "==marked== :smile: H~2~O"\n# comment\n---\nBody');
  expect(tree.toString()).toBe('Document(FrontMatter,Paragraph)');
});

test.each(['---\ntitle: open', 'text\n\n---\ntitle: later\n---', ' ---\ntitle: indented\n---', '```yaml\n---\ntitle: code\n---\n```', '---\na: '+ '中'.repeat(23000)+'\n---'])('non-header or oversized source stays Markdown', source => {
  expect(markdownParser.parse(source).toString()).not.toContain('FrontMatter');
});

test('incremental edits can remove and restore the first-line envelope', () => {
  let source = '---\ntitle: Example\n---\n# Body';
  let tree = markdownParser.parse(source);
  for (const insert of ['x', '']) {
    const next = insert + source.slice(insert ? 0 : 1);
    const change = { fromA: 0, toA: insert ? 0 : 1, fromB: 0, toB: insert.length };
    tree = markdownParser.parse(next, TreeFragment.applyChanges(TreeFragment.addTree(tree), [change]));
    expect(tree.toString()).toBe(markdownParser.parse(next).toString());
    source = next;
  }
});
