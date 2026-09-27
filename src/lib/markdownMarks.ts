import { Tree, type Input, type NodeSet, type NodeType, type ParseWrapper, type SyntaxNode } from '@lezer/common';
import { Tag, tags, styleTags } from '@lezer/highlight';
import { trackHtmlScope } from './markdownInline';

export const highlightTag = Tag.define();
export const subscriptTag = Tag.define();
export const superscriptTag = Tag.define();
export const markStyles = styleTags({
  'Highlight/...': highlightTag,
  'Subscript/...': subscriptTag,
  'Superscript/...': superscriptTag
});
export const markNodes = [
  { name: 'Highlight', style: highlightTag },
  { name: 'HighlightMark', style: tags.processingInstruction },
  { name: 'Subscript', style: subscriptTag },
  { name: 'SubscriptMark', style: tags.processingInstruction },
  { name: 'Superscript', style: superscriptTag },
  { name: 'SuperscriptMark', style: tags.processingInstruction }
];

const containers = new Set(['Paragraph', 'Task', 'TableCell', 'Emphasis', 'StrongEmphasis', 'Strikethrough', 'Link', 'Image',
  'ATXHeading1', 'ATXHeading2', 'ATXHeading3', 'ATXHeading4', 'ATXHeading5', 'ATXHeading6', 'SetextHeading1', 'SetextHeading2']);
const blocks = new Set(['Paragraph', 'Task', 'TableCell', 'ATXHeading1', 'ATXHeading2', 'ATXHeading3', 'ATXHeading4',
  'ATXHeading5', 'ATXHeading6', 'SetextHeading1', 'SetextHeading2']);
const opaque = new Set(['InlineCode', 'CodeBlock', 'FencedCode', 'HTMLBlock', 'CommentBlock', 'HTMLTag', 'Comment',
  'ProcessingInstruction', 'ProcessingInstructionBlock', 'URL', 'Autolink', 'MathSource', 'Highlight', 'Subscript', 'Superscript', 'LinkReference']);

interface Child { node: SyntaxNode; changed?: Tree }
type Kind = 'Highlight' | 'Subscript' | 'Superscript';
interface Mark { at: number; opening: boolean; kind: Kind }
interface Scope { from: number; to: number; whitespace?: number[] }
const width = (kind: Kind) => kind === 'Highlight' ? 2 : 1;
const isSpace = (text: string) => /\p{White_Space}/u.test(text);

function scriptFits(input: Input, offset: number, scope: Scope, from: number, to: number) {
  if (!scope.whitespace) {
    scope.whitespace = [];
    const text = input.read(scope.from + offset, scope.to + offset);
    let slashes = 0;
    for (let at = 0; at < text.length; at++) {
      const char = text[at];
      if (isSpace(char) && !(char === ' ' && slashes % 2)) scope.whitespace.push(scope.from + at);
      slashes = char === '\\' ? slashes + 1 : 0;
    }
  }
  let low = 0, high = scope.whitespace.length;
  while (low < high) {
    const mid = (low + high) >>> 1;
    if (scope.whitespace[mid] < from) low = mid + 1;
    else high = mid;
  }
  return low === scope.whitespace.length || scope.whitespace[low] >= to;
}
interface Frame {
  node: SyntaxNode;
  child: SyntaxNode | null;
  scanned: number;
  children: Child[];
  opens: Array<{ at: number; kind: Kind }>;
  scope: Scope;
  marks: Mark[];
}

function scan(input: Input, offset: number, frame: Frame, to: number, html: string[]) {
  const from = frame.scanned;
  frame.scanned = to;
  if (html.length || !containers.has(frame.node.name) || from >= to) return;
  const text = input.read(from + offset, to + offset);
  for (let index = 0; index < text.length; index++) {
    const char = text[index];
    const kind: Kind | null = char === '=' && text[index + 1] === '=' ? 'Highlight'
      : char === '~' ? 'Subscript' : char === '^' ? 'Superscript' : null;
    if (!kind) continue;
    const size = width(kind), at = from + index;
    const before = input.read(Math.max(0, at + offset - 1), at + offset);
    const after = input.read(at + offset + size, Math.min(input.length, at + offset + size + 1));
    if (before === char || after === char) continue;
    let opening = frame.opens.length - 1;
    while (opening >= 0 && frame.opens[opening].kind !== kind) opening--;
    let paired = false;
    if (before && !isSpace(before) && opening >= 0) {
      const start = frame.opens[opening].at;
      if (kind === 'Highlight' || scriptFits(input, offset, frame.scope, start + size, at)) {
        frame.marks.push({ at: start, opening: true, kind }, { at, opening: false, kind });
        frame.opens.length = opening;
        paired = true;
      } else frame.opens = frame.opens.filter(candidate => candidate.kind !== kind);
    }
    if (!paired && after && !isSpace(after)) frame.opens.push({ at, kind });
    index += size - 1;
  }
}

function assemble(frame: Frame, types: Record<Kind, [NodeType, NodeType]>): Tree | undefined {
  if (!frame.marks.length && !frame.children.some(child => child.changed)) return undefined;
  const marks = frame.marks.sort((a, b) => a.at - b.at);
  const groups: Array<{ from: number; trees: Tree[]; positions: number[] }> = [
    { from: frame.node.from, trees: [], positions: [] }
  ];
  const append = (tree: Tree, from: number) => {
    const group = groups[groups.length - 1];
    group.trees.push(tree);
    group.positions.push(from - group.from);
  };
  let childIndex = 0;
  for (const mark of marks) {
    const [type, marker] = types[mark.kind], size = width(mark.kind);
    while (childIndex < frame.children.length && frame.children[childIndex].node.from < mark.at) {
      const child = frame.children[childIndex++];
      append(child.changed ?? child.node.toTree(), child.node.from);
    }
    if (mark.opening) groups.push({ from: mark.at, trees: [], positions: [] });
    append(new Tree(marker, [], [], size), mark.at);
    if (!mark.opening) {
      const group = groups.pop()!;
      append(new Tree(type, group.trees, group.positions, mark.at + size - group.from), group.from);
    }
  }
  for (; childIndex < frame.children.length; childIndex++) {
    const child = frame.children[childIndex];
    append(child.changed ?? child.node.toTree(), child.node.from);
  }
  const group = groups[0];
  return new Tree(frame.node.type, group.trees, group.positions, frame.node.to - frame.node.from,
    frame.node.toTree().propValues);
}

// Process the existing tree, not a second parse or a replacement source string.
// Only parents of changed nodes are rebuilt. Native parent boundaries also
// reject crossing emphasis/link delimiters, matching the Rust event pass.
function highlightTree(tree: Tree, input: Input, offset: number, types: Record<Kind, [NodeType, NodeType]>,
  blocksCache: WeakMap<Tree, Tree>): Tree {
  const html: string[] = [];
  const enter = (node: SyntaxNode, scope: Scope = { from: node.from, to: node.to }): Frame => ({ node, child: node.firstChild, scanned: node.from,
    children: [], opens: [], marks: [], scope });
  const stack = [enter(tree.topNode)];
  while (stack.length) {
    const frame = stack[stack.length - 1];
    const child = frame.child;
    if (child) {
      scan(input, offset, frame, child.from, html);
      frame.scanned = child.to;
      frame.child = child.nextSibling;
      if (child.name === 'HTMLTag') trackHtmlScope(input.read(child.from + offset, child.to + offset), html);
      const cached = child.type.is('Block') && child.tree ? blocksCache.get(child.tree) : undefined;
      if (cached) {
        html.length = 0;
        frame.children.push({ node: child, changed: cached === child.tree ? undefined : cached });
      } else if (opaque.has(child.name) || !child.firstChild && !containers.has(child.name)) {
        frame.children.push({ node: child });
      } else {
        if (blocks.has(child.name)) html.length = 0;
        stack.push(enter(child, blocks.has(child.name) ? undefined : frame.scope));
      }
    } else {
      scan(input, offset, frame, frame.node.to, html);
      const changed = assemble(frame, types);
      if (frame.node.type.is('Block') && frame.node.tree) {
        const result = changed ?? frame.node.tree;
        blocksCache.set(frame.node.tree, result);
        blocksCache.set(result, result);
      }
      if (blocks.has(frame.node.name)) html.length = 0;
      stack.pop();
      if (!stack.length) return changed ?? tree;
      stack[stack.length - 1].children.push({ node: frame.node, changed });
    }
  }
  return tree;
}

export function marksWrapper(nodes: NodeSet): ParseWrapper {
  const types = Object.fromEntries(['Highlight', 'Subscript', 'Superscript'].map(name => [name,
    [nodes.types.find(type => type.name === name)!, nodes.types.find(type => type.name === name + 'Mark')!]
  ])) as Record<Kind, [NodeType, NodeType]>;
  // Immutable reused blocks are already processed. Do not rescan an entire
  // large document on each keystroke; weak keys follow the parser's lifetime.
  const blocksCache = new WeakMap<Tree, Tree>();
  return (inner, input, _fragments, ranges) => ({
    get parsedPos() { return inner.parsedPos; },
    get stoppedAt() { return inner.stoppedAt; },
    stopAt(pos) { inner.stopAt(pos); },
    advance() {
      const tree = inner.advance();
      return tree ? highlightTree(tree, input, ranges[0]?.from ?? 0, types, blocksCache) : null;
    }
  });
}
