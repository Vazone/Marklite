import { history, undoDepth } from '@codemirror/commands';
import { EditorSelection, EditorState } from '@codemirror/state';
import { afterEach, describe, expect, test, vi } from 'vitest';
import {
  createEditorSessionSnapshotController,
  createEditorState,
  serializeEditorState
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

    const restored = createEditorState('one two', [history()], serializeEditorState(state));
    const serialized = serializeEditorState(state);

    expect(restored.doc.toString()).toBe('one two');
    expect(restored.selection.main.from).toBe(0);
    expect(restored.selection.main.to).toBe(3);
    expect(undoDepth(restored)).toBe(1);
    expect(serialized).not.toHaveProperty('doc');
  });

  test('falls back to a clean state for mismatched or corrupt snapshots', () => {
    const corrupt = createEditorState('current', [history()], { selection: 'invalid' });

    expect(corrupt.doc.toString()).toBe('current');
    expect(undoDepth(corrupt)).toBe(0);
  });

  test('coalesces hot-path requests and serializes only the latest state after the delay', () => {
    vi.useFakeTimers();
    const first = EditorState.create({ doc: 'first', extensions: [history()] });
    const latest = EditorState.create({ doc: 'latest', extensions: [history()] });
    const emit = vi.fn();
    const serialize = vi.fn(serializeEditorState);
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
    const serialize = vi.fn(serializeEditorState);
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
