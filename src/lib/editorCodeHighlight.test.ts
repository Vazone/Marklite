import { EditorState } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { history, undo } from '@codemirror/commands';
import { afterEach, expect, test, vi } from 'vitest';
import { editorMarkdownLanguage } from './editorMarkdownLanguage';
import { editorCodeHighlight, visibleCodeBlocks } from './editorCodeHighlight';
import { createCodeHighlightCache } from './codeHighlightCache';

let view: EditorView | undefined;
afterEach(() => { view?.destroy(); view = undefined; document.body.replaceChildren(); });
const source = '# 标题😀\n\n```ts\nconst name = "你好";\n```\n\n```unknown\nignore\n```\n';

test('extracts visible fenced code in original UTF-16 positions', () => {
  const state = EditorState.create({ doc: source, extensions: [editorMarkdownLanguage()] });
  const blocks = visibleCodeBlocks(state, [{ from: 0, to: source.length }]);
  expect(blocks).toEqual([{ from: source.indexOf('const'), source: 'const name = "你好";', language: 'TypeScript' }]);
  expect(visibleCodeBlocks(state, [{ from: 0, to: 5 }])).toEqual([]);
});

test('oversized code and prose markers do not schedule language parsing', () => {
  const doc = '`javascript`\n\n```js\n' + 'x'.repeat(65537) + '\n```';
  const state = EditorState.create({ doc, extensions: [editorMarkdownLanguage()] });
  expect(visibleCodeBlocks(state, [{ from: 0, to: doc.length }])).toEqual([]);
});

test('async color decorations leave document, selection and undo intact', async () => {
  const run = vi.fn(async () => [{ from: 0, to: 5, classes: 'tok-keyword' }]);
  const parent = document.createElement('div'); document.body.append(parent);
  view = new EditorView({ parent, state: EditorState.create({ doc: source,
    selection: { anchor: source.indexOf('name') },
    extensions: [history(), editorMarkdownLanguage(), editorCodeHighlight(() => createCodeHighlightCache(run))] }) });
  const before = view.state.selection.main.anchor;
  await vi.waitFor(() => expect(parent.querySelector('.tok-keyword')?.textContent).toBe('const'));
  expect(view.state.doc.toString()).toBe(source);
  expect(view.state.selection.main.anchor).toBe(before);
  view.dispatch({ changes: { from: before, insert: 'new' } });
  expect(undo(view)).toBe(true);
  expect(view.state.doc.toString()).toBe(source);
});

test('document changes abort old code results', async () => {
  let complete!: (tokens: { from: number; to: number; classes: string }[]) => void;
  let signal!: AbortSignal;
  const parent = document.createElement('div'); document.body.append(parent);
  view = new EditorView({ parent, state: EditorState.create({ doc: source,
    extensions: [editorMarkdownLanguage(), editorCodeHighlight(() => createCodeHighlightCache(async (_source, _language, abort) => {
      signal = abort;
      return new Promise(resolve => { complete = resolve; });
    }))] }) });
  await vi.waitFor(() => expect(complete).toBeTypeOf('function'));
  view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: 'plain' } });
  expect(signal.aborted).toBe(true);
  complete([{ from: 0, to: 5, classes: 'tok-keyword' }]);
  await new Promise(resolve => setTimeout(resolve, 80));
  expect(parent.querySelector('.tok-keyword')).toBeNull();
  expect(view.state.doc.toString()).toBe('plain');
});
