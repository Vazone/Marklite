import { syntaxTree } from '@codemirror/language';
import { StateEffect, type EditorState, type Range } from '@codemirror/state';
import { Decoration, ViewPlugin, type DecorationSet, type EditorView, type ViewUpdate } from '@codemirror/view';
import { createCodeHighlightCache } from './codeHighlightCache';
import { canHighlightCode, CODE_LIMITS } from './codeTokens';
import { findCodeLanguage } from './codeLanguages';

type CodeBlock = { from: number; source: string; language: string };

export function visibleCodeBlocks(state: EditorState, ranges: readonly { from: number; to: number }[]): CodeBlock[] {
  const blocks: CodeBlock[] = [];
  const seen = new Set<number>();
  let units = 0;
  for (const range of ranges) syntaxTree(state).iterate({
    ...range,
    enter(node) {
      if (blocks.length >= 64) return false;
      if (node.name !== 'FencedCode') return;
      if (seen.has(node.from)) return false;
      seen.add(node.from);
      const info = node.node.getChild('CodeInfo'), code = node.node.getChild('CodeText');
      if (!info || !code || code.to - code.from > CODE_LIMITS.block || units + code.to - code.from > 262144) return false;
      const language = findCodeLanguage(state.sliceDoc(info.from, info.to))?.name;
      if (!language) return false;
      const source = state.sliceDoc(code.from, code.to);
      if (!canHighlightCode(source)) return false;
      blocks.push({ from: code.from, source, language });
      units += source.length;
      return false;
    }
  });
  return blocks;
}

const ready = StateEffect.define<DecorationSet>();

// Use view decorations rather than installing language parsers into the raw
// Markdown worker protocol. Code parsing stays cancellable and off-thread.
export function editorCodeHighlight(cacheFactory = createCodeHighlightCache) {
  return ViewPlugin.fromClass(class {
    decorations = Decoration.none;
    cache = cacheFactory();
    active = new AbortController();
    timer: ReturnType<typeof setTimeout> | undefined;
    destroyed = false;

    constructor(readonly view: EditorView) { this.schedule(); }
    update(update: ViewUpdate) {
      for (const transaction of update.transactions) for (const effect of transaction.effects) {
        if (effect.is(ready)) this.decorations = effect.value;
      }
      if (update.docChanged || update.viewportChanged || syntaxTree(update.startState) !== syntaxTree(update.state)) {
        this.decorations = Decoration.none;
        this.schedule();
      }
    }
    schedule() {
      this.active.abort();
      this.active = new AbortController();
      clearTimeout(this.timer);
      this.timer = setTimeout(() => { void this.render(this.active.signal); }, 50);
    }
    async render(signal: AbortSignal) {
      const doc = this.view.state.doc;
      const blocks = visibleCodeBlocks(this.view.state, this.view.visibleRanges);
      const ranges: Range<Decoration>[] = [];
      try {
        for (const block of blocks) {
          const tokens = await this.cache.get(block.source, block.language, signal);
          if (signal.aborted || this.destroyed || this.view.state.doc !== doc) return;
          if (ranges.length + tokens.length > CODE_LIMITS.tokens) continue;
          for (const token of tokens) ranges.push(Decoration.mark({ class: token.classes })
            .range(block.from + token.from, block.from + token.to));
        }
        if (!signal.aborted && !this.destroyed && this.view.state.doc === doc) {
          this.view.dispatch({ effects: ready.of(Decoration.set(ranges, true)) });
        }
      } catch (error) {
        console.warn('Editor code highlighting:', error);
      }
    }
    destroy() {
      this.destroyed = true;
      clearTimeout(this.timer);
      this.active.abort();
      this.cache.clear();
    }
  }, { decorations: plugin => plugin.decorations });
}
