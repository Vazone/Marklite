<script lang="ts">
  import type { CursorPosition, EditorScrollPosition } from '../app/stores/documentStore';
  import type { SerializedEditorState } from '../lib/editorSession';
  import type { ToolbarAction } from '../lib/markdownToolbar';
  import type { AppSettings } from '../lib/tauriApi';

  export let tabId: string;
  export let value = '';
  export let settings: AppSettings;
  export let serializedState: SerializedEditorState | null = null;
  export let initialScrollPosition: EditorScrollPosition;
  export let onChange: (tabId: string, value: string, lineCount: number) => void;
  export let onDirty: (tabId: string) => void;
  export let onCursorChange: (tabId: string, position: CursorPosition) => void;
  export let onScrollSync: (tabId: string, position: EditorScrollPosition, userInitiated: boolean) => void;
  export let onSessionChange: (tabId: string, state: SerializedEditorState) => void;

  $: callbackCount = [onChange, onDirty, onCursorChange, onScrollSync, onSessionChange].length;

  export function applyMarkdown(_action: ToolbarAction) {}
  export function jumpToLine(_line: number) {}
  export function openFind() {}
  export function openReplace() {}
  export function flushContent() {}
</script>

<div
  data-testid="lazy-editor"
  data-tab-id={tabId}
  data-value-length={value.length}
  data-theme={settings.theme}
  data-restored-session={serializedState ? 'true' : 'false'}
  data-initial-line={initialScrollPosition.line}
  data-callback-count={callbackCount}
></div>
