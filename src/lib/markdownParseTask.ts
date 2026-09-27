import { ParseContext } from '@codemirror/language';
import type { Text } from '@codemirror/state';
import type { NodeSet, ParseWrapper, Tree } from '@lezer/common';
import { editorText } from './editorText';
import { unpackMarkdownTree, type MarkdownTree } from './markdownTree';

const LONG_LINE = 64 * 1024;

export function createMarkdownParseTask(nodes: NodeSet, createWorker = () =>
  new Worker(new URL('./markdownParse.worker.ts', import.meta.url), { type: 'module' })) {
  let document: Text | undefined;
  let background = false;
  let tree: Tree | undefined;
  let pending: Promise<void> | undefined;
  let finish: (() => void) | undefined;
  let worker: Worker | undefined;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let failed = false;
  let disposed = false;

  function cancel() {
    if (timer !== undefined) clearTimeout(timer);
    timer = undefined;
    worker?.terminate();
    worker = undefined;
    finish?.();
    finish = undefined;
    pending = undefined;
    tree = undefined;
  }

  const wrap: ParseWrapper = (inner, input, fragments, ranges) => {
    const doc = ParseContext.get()?.state.doc;
    if (disposed || !doc) return inner;
    if (document !== doc) {
      cancel();
      document = doc;
      background = false;
      // Text iteration uses existing line strings, without flattening the file.
      if (doc.length > LONG_LINE) {
        for (const line of doc.iterLines()) if (line.length > LONG_LINE) { background = true; break; }
      }
      failed = false;
    }
    if (!background || failed || ranges.length !== 1 || ranges[0].from !== 0 || ranges[0].to !== doc.length) return inner;
    if (tree) {
      const result = tree;
      return { parsedPos: doc.length, stoppedAt: null, stopAt() {}, advance: () => result };
    }
    if (!pending) {
      pending = new Promise(resolve => { finish = resolve; });
      const requested = doc;
      timer = setTimeout(() => {
        timer = undefined;
        const fail = (message: string) => {
          if (disposed || document !== requested) return;
          failed = true;
          console.warn(`Background Markdown parser: ${message}`);
          worker?.terminate();
          worker = undefined;
          finish?.();
          finish = undefined;
        };
        try {
          const active = worker = createWorker();
          active.onmessage = (event: MessageEvent<{ tree?: MarkdownTree; error?: string }>) => {
            if (disposed || document !== requested || worker !== active) return;
            if (!event.data.tree) { fail(event.data.error ?? 'No syntax tree returned'); return; }
            try { tree = unpackMarkdownTree(event.data.tree, nodes); }
            catch (error) { fail(String(error)); return; }
            active.terminate();
            worker = undefined;
            finish?.();
            finish = undefined;
          };
          active.onerror = event => { event.preventDefault(); fail(event.message); };
          active.postMessage(editorText(requested));
        } catch (error) { fail(String(error)); }
      }, 0);
    }
    return ParseContext.getSkippingParser(pending).startParse(input, fragments, ranges);
  };

  return { wrap, dispose() { disposed = true; cancel(); document = undefined; } };
}
