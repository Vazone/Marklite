import { history, undo, redo, undoDepth } from '@codemirror/commands';
import { EditorSelection, EditorState } from '@codemirror/state';
import { afterEach, describe, expect, test, vi } from 'vitest';
import {
  createEditorSessionSnapshotController,
  createEditorState,
  snapshotEditorState
} from './editorSession';

afterEach(() => {
  vi.useRealTimers();
});

describe('CodeMirror editor session snapshots', () => {
  test('restores document, selection and undo history with fresh extensions', () => {
    let state = EditorState.create({ doc: 'one', extensions: [history()] });
    state = state.update({
      changes: { from: 3, insert: ' two' },
      selection: EditorSelection.range(0, 3)
    }).state;
    expect(undoDepth(state)).toBe(1);

    const restored = createEditorState('one two', [history()], snapshotEditorState(state));
    const serialized = snapshotEditorState(state);

    expect(restored.doc.toString()).toBe('one two');
    expect(restored.selection.main.from).toBe(0);
    expect(restored.selection.main.to).toBe(3);
    expect(undoDepth(restored)).toBe(1);
    expect(serialized).not.toHaveProperty('doc');
  });

  test('falls back to a clean state when a stale selection exceeds the document', () => {
    const corrupt = createEditorState('current', [history()], { selection: EditorSelection.single(100) });

    expect(corrupt.doc.toString()).toBe('current');
    expect(undoDepth(corrupt)).toBe(0);
  });

  test('does not flatten or serialize content and preserves working undo/redo', () => {
    const original = EditorState.create({ doc: 'one', extensions: [history()] });
    const edited = original.update({ changes: { from: 3, insert: ' two' } }).state;
    const flatten = vi.spyOn(edited, 'sliceDoc');
    const serialize = vi.spyOn(edited, 'toJSON');
    const snapshot = snapshotEditorState(edited);
    expect(snapshot.selection).toBe(edited.selection);
    expect(flatten).not.toHaveBeenCalled();
    expect(serialize).not.toHaveBeenCalled();
    let restored = createEditorState('one two', [history()], snapshot);
    const target = { get state() { return restored; }, dispatch: (tr: { state: EditorState }) => { restored = tr.state; } };
    expect(undo(target)).toBe(true);
    expect(restored.doc.toString()).toBe('one');
    expect(redo(target)).toBe(true);
    expect(restored.doc.toString()).toBe('one two');
  });

  test('coalesces hot-path requests and serializes only the latest state after the delay', () => {
    vi.useFakeTimers();
    const first = EditorState.create({ doc: 'first', extensions: [history()] });
    const latest = EditorState.create({ doc: 'latest', extensions: [history()] });
    const emit = vi.fn();
    const serialize = vi.fn(snapshotEditorState);
    const controller = createEditorSessionSnapshotController(emit, { delayMs: 250, serialize });

    controller.request(first);
    controller.request(latest);

    expect(serialize).not.toHaveBeenCalled();
    expect(emit).not.toHaveBeenCalled();

    vi.advanceTimersByTime(249);
    expect(serialize).not.toHaveBeenCalled();

    vi.advanceTimersByTime(1);
    expect(serialize).toHaveBeenCalledTimes(1);
    expect(serialize).toHaveBeenCalledWith(latest);
    expect(emit).toHaveBeenCalledTimes(1);
    expect(emit.mock.calls[0][0]).not.toHaveProperty('doc');
  });

  test('flushes the exact latest state and cancel prevents a stale delayed emission', () => {
    vi.useFakeTimers();
    const first = EditorState.create({ doc: 'first', extensions: [history()] });
    const latest = EditorState.create({ doc: 'latest', extensions: [history()] });
    const emit = vi.fn();
    const serialize = vi.fn(snapshotEditorState);
    const controller = createEditorSessionSnapshotController(emit, { delayMs: 250, serialize });

    controller.request(first);
    controller.flush(latest);

    expect(serialize).toHaveBeenCalledTimes(1);
    expect(serialize).toHaveBeenCalledWith(latest);
    expect(emit).toHaveBeenCalledTimes(1);

    controller.request(first);
    controller.cancel();
    vi.runAllTimers();

    expect(serialize).toHaveBeenCalledTimes(1);
    expect(emit).toHaveBeenCalledTimes(1);
  });
});
