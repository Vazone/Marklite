import type { Text } from '@codemirror/state';

const strings = new WeakMap<Text, string>();

/** Share the materialized string only while its immutable document is alive. */
export function editorText(doc: Text): string {
  let value = strings.get(doc);
  if (value === undefined) {
    value = doc.toString();
    strings.set(doc, value);
  }
  return value;
}
