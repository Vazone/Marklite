import { historyField } from '@codemirror/commands';
import { EditorState, type Extension } from '@codemirror/state';

export type SerializedEditorState = ReturnType<EditorState['toJSON']>;

export function serializeEditorState(state: EditorState): SerializedEditorState {
  return state.toJSON({ history: historyField });
}

export function createEditorState(
  doc: string,
  extensions: Extension,
  serialized: SerializedEditorState | null
): EditorState {
  if (serialized && typeof serialized === 'object' && serialized.doc === doc) {
    try {
      return EditorState.fromJSON(serialized, { extensions }, { history: historyField });
    } catch {
      // A snapshot is process-local cache. Fall back instead of blocking the editor.
    }
  }

  return EditorState.create({ doc, extensions });
}
