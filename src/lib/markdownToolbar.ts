import {
  Bold,
  Code,
  Heading1,
  Heading2,
  Heading3,
  Image,
  Italic,
  Link,
  List,
  ListChecks,
  ListOrdered,
  Minus,
  Quote,
  Strikethrough,
  Table,
  TextCursorInput
} from 'lucide-svelte';
import { shortcutFor, type CommandId } from './commands';
import type { MessageKey } from './i18n/messages';
export type ToolbarAction =
  | 'bold'
  | 'italic'
  | 'strike'
  | 'h1'
  | 'h2'
  | 'h3'
  | 'quote'
  | 'codeBlock'
  | 'inlineCode'
  | 'unorderedList'
  | 'orderedList'
  | 'taskList'
  | 'link'
  | 'image'
  | 'table'
  | 'hr';

export type ToolbarItem = {
  action: ToolbarAction;
  labelKey: MessageKey;
  shortcut?: string;
  icon: typeof Bold;
  commandId?: CommandId;
};

export const toolbarItems: ToolbarItem[] = [
  { action: 'bold', labelKey: 'toolbar.bold', shortcut: shortcutFor('format-bold'), commandId: 'format-bold', icon: Bold },
  { action: 'italic', labelKey: 'toolbar.italic', shortcut: shortcutFor('format-italic'), commandId: 'format-italic', icon: Italic },
  { action: 'strike', labelKey: 'toolbar.strike', icon: Strikethrough },
  { action: 'h1', labelKey: 'toolbar.h1', icon: Heading1 },
  { action: 'h2', labelKey: 'toolbar.h2', icon: Heading2 },
  { action: 'h3', labelKey: 'toolbar.h3', icon: Heading3 },
  { action: 'quote', labelKey: 'toolbar.quote', icon: Quote },
  { action: 'codeBlock', labelKey: 'toolbar.codeBlock', icon: Code },
  { action: 'inlineCode', labelKey: 'toolbar.inlineCode', icon: TextCursorInput },
  { action: 'unorderedList', labelKey: 'toolbar.unorderedList', icon: List },
  { action: 'orderedList', labelKey: 'toolbar.orderedList', icon: ListOrdered },
  { action: 'taskList', labelKey: 'toolbar.taskList', icon: ListChecks },
  { action: 'link', labelKey: 'toolbar.link', shortcut: shortcutFor('insert-link'), commandId: 'insert-link', icon: Link },
  { action: 'image', labelKey: 'toolbar.image', icon: Image },
  { action: 'table', labelKey: 'toolbar.table', icon: Table },
  { action: 'hr', labelKey: 'toolbar.hr', icon: Minus }
];
