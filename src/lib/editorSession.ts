import { historyField } from '@codemirror/commands';
import { EditorState, type Extension } from '@codemirror/state';

export type SerializedEditorState = {
  selection?: unknown;
  history?: unknown;
};

type SnapshotControllerOptions = {
  delayMs?: number;
  serialize?: (state: EditorState) => SerializedEditorState;
};

export type EditorSessionSnapshotController = {
  request(state: EditorState): void;
  flush(state?: EditorState): void;
  cancel(): void;
};

export function serializeEditorState(state: EditorState): SerializedEditorState {
  const { selection, history } = state.toJSON({ history: historyField });
  return { selection, history };
}

export function createEditorState(
  doc: string,
  extensions: Extension,
  serialized: SerializedEditorState | null
): EditorState {
  if (serialized && typeof serialized === 'object') {
    try {
      return EditorState.fromJSON({ ...serialized, doc }, { extensions }, { history: historyField });
    } catch {
      // A snapshot is process-local cache. Fall back instead of blocking the editor.
    }
  }

  return EditorState.create({ doc, extensions });
}

export function createEditorSessionSnapshotController(
  emit: (state: SerializedEditorState) => void,
  options: SnapshotControllerOptions = {}
): EditorSessionSnapshotController {
  const delayMs = options.delayMs ?? 400;
  const serialize = options.serialize ?? serializeEditorState;
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
