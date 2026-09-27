import { historyField } from '@codemirror/commands';
import { EditorSelection, EditorState, type Extension } from '@codemirror/state';

export type EditorSnapshot = {
  selection: EditorSelection;
  history?: unknown;
};

type SnapshotControllerOptions = {
  delayMs?: number;
  serialize?: (state: EditorState) => EditorSnapshot;
};

export type EditorSessionSnapshotController = {
  request(state: EditorState): void;
  flush(state?: EditorState): void;
  cancel(): void;
};

export function snapshotEditorState(state: EditorState): EditorSnapshot {
  // This cache never crosses IPC or survives the process. Keep immutable values
  // directly: EditorState.toJSON also flattens the entire document before it can
  // discard `doc`, and serializes the undo history on every selection update.
  return { selection: state.selection, history: state.field(historyField, false) };
}

export function createEditorState(
  doc: string,
  extensions: Extension,
  serialized: EditorSnapshot | null
): EditorState {
  if (serialized && typeof serialized === 'object') {
    try {
      return EditorState.create({
        doc,
        selection: serialized.selection,
        extensions: serialized.history === undefined
          ? extensions
          : [extensions, historyField.init(() => serialized.history)]
      });
    } catch {
      // A snapshot is process-local cache. Fall back instead of blocking the editor.
    }
  }

  return EditorState.create({ doc, extensions });
}

export function createEditorSessionSnapshotController(
  emit: (state: EditorSnapshot) => void,
  options: SnapshotControllerOptions = {}
): EditorSessionSnapshotController {
  const delayMs = options.delayMs ?? 400;
  const serialize = options.serialize ?? snapshotEditorState;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let pendingState: EditorState | null = null;

  function clearTimer() {
    if (timer !== undefined) clearTimeout(timer);
    timer = undefined;
  }

  function emitPending() {
    clearTimer();
    const state = pendingState;
    pendingState = null;
    if (state) emit(serialize(state));
  }

  return {
    request(state) {
      pendingState = state;
      clearTimer();
      timer = setTimeout(emitPending, delayMs);
    },
    flush(state) {
      if (state) pendingState = state;
      emitPending();
    },
    cancel() {
      clearTimer();
      pendingState = null;
    }
  };
}
