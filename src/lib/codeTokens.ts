import type { Parser } from '@lezer/common';
import { classHighlighter, highlightTree } from '@lezer/highlight';

export const CODE_LIMITS = { block: 65536, line: 16384, tokens: 8192, milliseconds: 100 } as const;
export type CodeToken = { from: number; to: number; classes: string };

export function canHighlightCode(source: string): boolean {
  if (source.length > CODE_LIMITS.block) return false;
  let start = 0;
  for (let at = 0; at <= source.length; at++) {
    if (at - start > CODE_LIMITS.line) return false;
    if (source[at] === '\n' || source[at] === '\r') start = at + 1;
  }
  return true;
}

// Run in the code worker. A worker watchdog must also enforce the deadline:
// an individual third-party parser advance() cannot be preempted in this loop.
export function parseCodeTokens(parser: Parser, source: string): CodeToken[] {
  if (!canHighlightCode(source)) return [];
  const deadline = performance.now() + CODE_LIMITS.milliseconds;
  const parse = parser.startParse(source);
  let tree;
  do {
    tree = parse.advance();
    if (performance.now() > deadline) return [];
  } while (!tree);
  const tokens: CodeToken[] = [];
  const exhausted = {};
  try {
    highlightTree(tree, classHighlighter, (from, to, classes) => {
      if (tokens.length >= CODE_LIMITS.tokens || performance.now() > deadline) throw exhausted;
      tokens.push({ from, to, classes });
    });
  } catch (error) {
    if (error !== exhausted) throw error;
    return [];
  }
  return tokens;
}
