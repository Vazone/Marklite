import { NodeProp, NodeType, Tree, TreeBuffer, type NodeSet } from '@lezer/common';

// Only raw Markdown trees cross this boundary. HTML/code mixed parsing remains
// in CodeMirror's outer parseCode wrapper and keeps its original node types.
export type MarkdownTree = {
  type: number;
  length: number;
  positions: readonly number[];
  children: Array<MarkdownTree | { buffer: Uint16Array; length: number }>;
  contextHash?: number;
};

export function packMarkdownTree(tree: Tree): MarkdownTree {
  return {
    type: tree.type === NodeType.none ? -1 : tree.type.id,
    length: tree.length,
    positions: tree.positions,
    contextHash: tree.prop(NodeProp.contextHash),
    children: tree.children.map(child => child instanceof TreeBuffer
      ? { buffer: child.buffer, length: child.length }
      : packMarkdownTree(child))
  };
}

export function unpackMarkdownTree(tree: MarkdownTree, nodes: NodeSet): Tree {
  return new Tree(tree.type < 0 ? NodeType.none : nodes.types[tree.type],
    tree.children.map(child => 'buffer' in child
      ? new TreeBuffer(child.buffer, child.length, nodes)
      : unpackMarkdownTree(child, nodes)),
    tree.positions, tree.length,
    tree.contextHash === undefined ? undefined : [[NodeProp.contextHash, tree.contextHash]]);
}
