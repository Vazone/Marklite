import { expect, test } from 'vitest';
import { EditorState } from '@codemirror/state';
import { history, undo } from '@codemirror/commands';
import { editorMarkdownLanguage } from './editorMarkdownLanguage';
import fixtures from '../shared/markdown-inline-fixtures.json';
import registry from '../shared/emoji-registry.json';

const aliases: Record<string, string> = registry.aliases;

test.each(fixtures)('shared inline protection: $id', ({ source, emojis }) => {
  const found: string[] = [];
  editorMarkdownLanguage().language.parser.parse(source).iterate({ enter(node) {
    if (node.name === 'Emoji') found.push(aliases[source.slice(node.from + 1, node.to - 1)]);
  } });
  expect(found).toEqual(emojis);
});

test('every registry alias has its original UTF-16 syntax range', () => {
  const parser = editorMarkdownLanguage().language.parser;
  for (const alias of Object.keys(aliases)) {
    const source = `中文😀 :${alias}:`;
    const node = parser.parse(source).topNode.firstChild?.getChild('Emoji');
    expect(node?.from, alias).toBe(5);
    expect(node?.to, alias).toBe(source.length);
    expect(source.slice(node!.from, node!.to)).toBe(`:${alias}:`);
  }
});

test('syntax recognition preserves Markdown and one edit undoes completely', () => {
  const original = ':unknown:';
  let state = EditorState.create({ doc: original, extensions: [editorMarkdownLanguage(), history()] });
  state = state.update({ changes: { from: 0, to: original.length, insert: '**:smile:**' } }).state;
  expect(state.doc.toString()).toBe('**:smile:**');
  expect(undo({ state, dispatch: transaction => { state = transaction.state; } })).toBe(true);
  expect(state.doc.toString()).toBe(original);
});
