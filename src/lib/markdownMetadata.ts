import { BlockContext, type MarkdownConfig } from '@lezer/markdown';
import type { Input } from '@lezer/common';
import { tags } from '@lezer/highlight';

const maxBytes = 64 * 1024;
const encoder = new TextEncoder();
const headers = new WeakMap<BlockContext, number>();

// Recognize the source envelope only. Rust owns YAML validation and diagnostics;
// the editor never interprets metadata values as Markdown or application config.
function headerEnd(input: Input): number {
  if (!/^\uFEFF?---/.test(input.read(0, Math.min(input.length, 4)))) return 0;
  const source = input.read(0, Math.min(input.length, maxBytes + 4));
  let at = 0, bytes = 0, first = true;
  for (const match of source.matchAll(/[^\r\n]*(?:\r\n|\r|\n|$)/g)) {
    if (!match[0]) break;
    const raw = match[0];
    bytes += encoder.encode(raw).length;
    if (bytes > maxBytes) return 0;
    at += raw.length;
    const line = raw.replace(/[\r\n]+$/, '').replace(/[ \t]+$/, '');
    if (first) {
      if (line.replace(/^\uFEFF/, '') !== '---') return 0;
      first = false;
    } else if (line === '---' || line === '...') return at;
  }
  return 0;
}

export const markdownMetadata: MarkdownConfig = {
  defineNodes: [{ name: 'FrontMatter', block: true, style: tags.meta }],
  wrap(inner, input, _fragments, ranges) {
    // This extension runs before other wrappers, so the public BlockContext is
    // the parse object also received by the block rule. No private fields needed.
    if (inner instanceof BlockContext && ranges[0]?.from === 0) headers.set(inner, headerEnd(input));
    return inner;
  },
  parseBlock: [{
    name: 'FrontMatter', before: 'HorizontalRule',
    parse(cx) {
      const end = headers.get(cx) ?? 0;
      if (!end || cx.lineStart !== 0 || cx.depth !== 1) return false;
      do { if (!cx.nextLine()) break; } while (cx.lineStart < end);
      cx.addElement(cx.elt('FrontMatter', 0, end));
      return true;
    }
  }]
};
