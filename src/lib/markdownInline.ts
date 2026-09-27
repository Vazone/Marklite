import { Emoji, parser, type InlineContext, type MarkdownConfig } from '@lezer/markdown';
import registry from '../shared/emoji-registry.json';
import { markdownMathRanges } from './markdownMathRanges';

const htmlType = parser.nodeSet.types.find(type => type.name === 'HTMLTag')!.id;
const voidTags = new Set(['area', 'base', 'br', 'col', 'embed', 'hr', 'img', 'input', 'link', 'meta', 'param', 'source', 'track', 'wbr']);
const htmlScopes = new WeakMap<InlineContext, string[]>();
const mathScopes = new WeakMap<InlineContext, Map<number, number>>();

// Ask the existing parser to validate the tag. No second HTML grammar or private
// Lezer fields; quoted '>' characters must not prematurely end the candidate.
function htmlTag(cx: InlineContext, pos: number): number {
  if (!/^<\/?[A-Za-z]/.test(cx.slice(pos, pos + 3))) return -1;
  let quote = 0;
  let end = pos + 1;
  for (; end < cx.end; end++) {
    const char = cx.char(end);
    if (quote) { if (char === quote) quote = 0; }
    else if (char === 34 || char === 39) quote = char;
    else if (char === 60) return -1;
    else if (char === 62) break;
  }
  if (end === cx.end) return -1;
  const source = cx.slice(pos, ++end);
  const elements = parser.parseInline(source, pos);
  if (elements.length !== 1 || elements[0].type !== htmlType || elements[0].to !== end) return -1;
  const stack = htmlScopes.get(cx) ?? [];
  htmlScopes.set(cx, stack);
  trackHtmlScope(source, stack);
  return cx.addElement(cx.elt('HTMLTag', pos, end));
}

export function trackHtmlScope(source: string, stack: string[]) {
  const name = /^<\/?([A-Za-z][\w-]*)/.exec(source)?.[1].toLowerCase();
  if (!name) return;
  if (source.startsWith('</')) {
    const index = stack.lastIndexOf(name);
    if (index >= 0) stack.length = index;
  } else if (!/\/\s*>$/.test(source) && !voidTags.has(name)) stack.push(name);
}

export const markdownInline: MarkdownConfig = {
  defineNodes: [...Emoji.defineNodes!, 'MathSource'],
  parseInline: [
    { name: 'ProtectedHtml', before: 'HTMLTag',
      parse: (cx, next, pos) => next === 60 ? htmlTag(cx, pos) : -1 },
    { name: 'MathSource', before: 'InlineCode', parse(cx, next, pos) {
      if (next !== 36 || htmlScopes.get(cx)?.length) return -1;
      let ranges = mathScopes.get(cx);
      if (!ranges) { ranges = markdownMathRanges(cx.text); mathScopes.set(cx, ranges); }
      const end = ranges.get(pos - cx.offset);
      return end === undefined ? -1 : cx.addElement(cx.elt('MathSource', pos, end + cx.offset));
    } },
    { name: 'Emoji', parse(cx, next, pos) {
      if (next !== 58 || htmlScopes.get(cx)?.length) return -1;
      const match = /^([a-z0-9_+-]+):/.exec(cx.slice(pos + 1, Math.min(cx.end, pos + registry.maxShortcodeLength + 2)));
      if (!match || !Object.hasOwn(registry.aliases, match[1])) return -1;
      return cx.addElement(cx.elt('Emoji', pos, pos + match[0].length + 1));
    } }
  ]
};
