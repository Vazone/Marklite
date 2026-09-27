import { EditorState } from '@codemirror/state';
import { afterEach, expect, test, vi } from 'vitest';
import { findEditorSearchMatches } from './editorSearch';
import { startEditorSearch } from './editorSearchTask';

afterEach(() => vi.useRealTimers());

test.each([
  ['a'.repeat(100_000), 'aaa'],
  ['x'.repeat(32767) + '🪶A.B\n中文' + 'x'.repeat(70000), '🪶a.b'],
  ['x'.repeat(32766) + 'AB\nCD' + 'x'.repeat(40000), 'b\nc'],
  ['İx and X\n'.repeat(10000), 'x'],
  ['a'.repeat(100000), 'missing'],
  ['hello', '   ']
])('matches the synchronous reference across chunks %#', async (source, query) => {
  vi.useFakeTimers();
  const doc = EditorState.create({ doc: source }).doc;
  const complete = vi.fn();
  const flatten = vi.spyOn(doc, 'toString');
  startEditorSearch(doc, query, complete);
  expect(complete).not.toHaveBeenCalled();
  await vi.runAllTimersAsync();
  expect(complete).toHaveBeenCalledExactlyOnceWith(findEditorSearchMatches(doc, query));
  expect(flatten).not.toHaveBeenCalled();
});

test('yields dense collection and cancellation discards an obsolete document/query', async () => {
  vi.useFakeTimers();
  const oldDoc = EditorState.create({ doc: 'a'.repeat(150000) }).doc;
  const stale = vi.fn();
  const cancel = startEditorSearch(oldDoc, 'a', stale);
  await vi.advanceTimersToNextTimerAsync();
  expect(stale).not.toHaveBeenCalled();
  cancel();
  const latest = vi.fn();
  startEditorSearch(EditorState.create({ doc: 'latest' }).doc, 'late', latest);
  await vi.runAllTimersAsync();
  expect(stale).not.toHaveBeenCalled();
  expect(latest).toHaveBeenCalledExactlyOnceWith([{ from: 0, to: 4 }]);
});
