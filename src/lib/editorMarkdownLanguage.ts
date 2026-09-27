import { commonmarkLanguage, markdown } from '@codemirror/lang-markdown';
import { Language, HighlightStyle, defaultHighlightStyle } from '@codemirror/language';
import type { ParseWrapper } from '@lezer/common';
import { createMarkdownParseTask } from './markdownParseTask';
import { markdownParser } from './markdownParser';
import { highlightTag, subscriptTag, superscriptTag } from './markdownMarks';

export const editorHighlightStyle = HighlightStyle.define([
  ...defaultHighlightStyle.specs,
  { tag: highlightTag, backgroundColor: '#fff3a8', color: '#24292f' },
  { tag: subscriptTag, fontSize: '0.85em', position: 'relative', top: '0.25em' },
  { tag: superscriptTag, fontSize: '0.85em', position: 'relative', top: '-0.35em' }
]);

// Source stays unchanged; supported extensions only add syntax nodes.
export function editorMarkdownLanguage(wrap?: ParseWrapper) {
  // Install the raw-tree wrapper before markdown() appends parseCode. Combining
  // them in one extension array reverses wrapper order and bypasses nested HTML.
  const base = new Language(commonmarkLanguage.data, wrap ? markdownParser.configure({ wrap }) : markdownParser);
  return markdown({ base });
}

export function createEditorLanguage() {
  const task = createMarkdownParseTask(markdownParser.nodeSet);
  return { support: editorMarkdownLanguage(task.wrap), dispose: task.dispose };
}
