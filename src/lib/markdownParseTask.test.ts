import { ParseContext } from '@codemirror/language';
import { EditorState } from '@codemirror/state';
import { markdownParser } from './markdownParser';
import { afterEach, expect, test, vi } from 'vitest';
import { editorMarkdownLanguage } from './editorMarkdownLanguage';
import { createMarkdownParseTask } from './markdownParseTask';
import { packMarkdownTree } from './markdownTree';
import { editorText } from './editorText';

afterEach(() => { vi.restoreAllMocks(); vi.useRealTimers(); });

function setup(content: string) {
  vi.useFakeTimers();
  const raw = markdownParser;
  let state = EditorState.create({ doc: content });
  vi.spyOn(ParseContext, 'get').mockImplementation(() => ({ state }) as ParseContext);
  const workers: Array<{ onmessage: ((event: { data: unknown }) => void) | null; terminate: ReturnType<typeof vi.fn>; postMessage: ReturnType<typeof vi.fn> }> = [];
  const task = createMarkdownParseTask(raw.nodeSet, () => {
    const worker = { onmessage: null, terminate: vi.fn(), postMessage: vi.fn() };
    workers.push(worker);
    return worker as unknown as Worker;
  });
  const language = editorMarkdownLanguage(task.wrap).language;
  return { task, language, workers, raw, state, change: (value: string) => { state = EditorState.create({ doc: value }); } };
}

test('background tree preserves full GFM offsets and nested HTML parsing', async () => {
  const content = 'prefix '.repeat(12000) + '==**bold :smile:**== H~2~O x^2^ <span title="x">:smile:</span> ~~strike~~ [:+1:](url) $x^2 :smile:$';
  const { task, language, workers, raw } = setup(content);
  language.parser.parse(content);
  await vi.runAllTimersAsync();
  expect(workers).toHaveLength(1);
  expect(workers[0].postMessage).toHaveBeenCalledExactlyOnceWith(content);
  workers[0].onmessage!({ data: { tree: packMarkdownTree(raw.parse(content)) } });
  const result = language.parser.parse(content);
  const expected = editorMarkdownLanguage().language.parser.parse(content);
  const positions = (tree: typeof result) => {
    const nodes: unknown[] = [];
    tree.iterate({ enter: node => { nodes.push([node.name, node.from, node.to]); } });
    return nodes;
  };
  expect(positions(result)).toEqual(positions(expected));
  expect(result.toString()).toContain('StrongEmphasis');
  expect(result.toString().match(/Emoji/g)).toHaveLength(2);
  expect(result.toString()).toContain('MathSource');
    expect(result.toString().match(/Highlight\(/g)).toHaveLength(1);
    expect(result.toString().match(/Subscript\(/g)).toHaveLength(1);
    expect(result.toString().match(/Superscript\(/g)).toHaveLength(1);
  const attribute = content.indexOf('title=') + 2;
  expect(result.resolveInner(attribute).name).toBe(expected.resolveInner(attribute).name);
  expect(result.resolveInner(attribute).name).toBe('AttributeName');
  expect(workers[0].terminate).toHaveBeenCalledOnce();
  task.dispose();
});

test('new short document terminates old work and cannot accept its late tree', async () => {
  const content = 'a'.repeat(100000);
  const { task, language, workers, raw, change } = setup(content);
  language.parser.parse(content);
  await vi.runAllTimersAsync();
  change('**new**');
  language.parser.parse('**new**');
  expect(workers[0].terminate).toHaveBeenCalledOnce();
  workers[0].onmessage!({ data: { tree: packMarkdownTree(raw.parse(content)) } });
  expect(language.parser.parse('**new**').toString()).toBe('Document(Paragraph(StrongEmphasis(EmphasisMark,EmphasisMark)))');
  task.dispose();
});

test('dispose cancels queued startup and ordinary multi-line documents use the normal parser', async () => {
  const content = 'short paragraph\n\n'.repeat(10000);
  const { task, language, workers, change } = setup(content);
  expect(language.parser.parse(content).length).toBe(content.length);
  await vi.runAllTimersAsync();
  expect(workers).toHaveLength(0);
  change('a'.repeat(100000));
  language.parser.parse('a'.repeat(100000));
  task.dispose();
  await vi.runAllTimersAsync();
  expect(workers).toHaveLength(0);
});

test('immutable document materialization is shared between parsing and content flush', () => {
  const doc = EditorState.create({ doc: 'a\nb' }).doc;
  const flatten = vi.spyOn(doc, 'toString');
  expect(editorText(doc)).toBe('a\nb');
  expect(editorText(doc)).toBe('a\nb');
  expect(flatten).toHaveBeenCalledOnce();
});
