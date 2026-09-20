import { markdown } from '@codemirror/lang-markdown';
import { GFM } from '@lezer/markdown';

// The exporter uses GFM plus selected named extensions. CodeMirror parses GFM
// syntax here; unsupported shorthand must remain ordinary source text.
export function editorMarkdownLanguage() {
  return markdown({ extensions: GFM });
}
