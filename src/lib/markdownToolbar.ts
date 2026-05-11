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
  label: string;
  shortcut?: string;
  icon: typeof Bold;
};

export const toolbarItems: ToolbarItem[] = [
  { action: 'bold', label: '加粗', shortcut: 'Ctrl+B', icon: Bold },
  { action: 'italic', label: '斜体', shortcut: 'Ctrl+I', icon: Italic },
  { action: 'strike', label: '删除线', icon: Strikethrough },
  { action: 'h1', label: '一级标题', icon: Heading1 },
  { action: 'h2', label: '二级标题', icon: Heading2 },
  { action: 'h3', label: '三级标题', icon: Heading3 },
  { action: 'quote', label: '引用', icon: Quote },
  { action: 'codeBlock', label: '代码块', icon: Code },
  { action: 'inlineCode', label: '行内代码', icon: TextCursorInput },
  { action: 'unorderedList', label: '无序列表', icon: List },
  { action: 'orderedList', label: '有序列表', icon: ListOrdered },
  { action: 'taskList', label: '任务列表', icon: ListChecks },
  { action: 'link', label: '插入链接', shortcut: 'Ctrl+K', icon: Link },
  { action: 'image', label: '插入图片', icon: Image },
  { action: 'table', label: '插入表格', icon: Table },
  { action: 'hr', label: '水平分割线', icon: Minus }
];
