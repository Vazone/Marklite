import type { Text } from '@codemirror/state';
import { RegExpCursor } from '@codemirror/search';
import type { EditorSearchMatch } from './editorSearch';

// Bound both sparse scans and dense result collection. A single cursor.next()
// over the entire document could otherwise block even when there are no hits.
const CHUNK_SIZE = 32 * 1024;
const SLICE_MS = 4;
const MATCH_BATCH = 8192;

export function startEditorSearch(
  doc: Text,
  query: string,
  complete: (matches: EditorSearchMatch[]) => void
): () => void {
  const needle = query.trim();
  const pattern = needle.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  let matches: EditorSearchMatch[] = [];
  let offset = 0;
  let boundary = 0;
  let resume = 0;
  let cursor: RegExpCursor | null = null;
  let cancelled = false;
  let timer: ReturnType<typeof setTimeout>;

  function step() {
    if (cancelled) return;
    const started = performance.now();
    let collected = 0;
    while (needle && offset < doc.length) {
      if (!cursor) {
        boundary = Math.min(doc.length, offset + CHUNK_SIZE);
        // Do not start a new cursor in the middle of a Unicode character.
        while (boundary < doc.length && /[\uDC00-\uDFFF]/.test(doc.sliceString(boundary, boundary + 1))) boundary++;
        const end = Math.min(doc.length, boundary + needle.length + 2);
        cursor = new RegExpCursor(doc.slice(offset, end), pattern, { ignoreCase: true });
        resume = boundary;
      }
      const next = cursor.next();
      if (next.done || offset + next.value.from >= boundary) {
        offset = resume;
        cursor = null;
      } else {
        const from = offset + next.value.from;
        const to = offset + next.value.to;
        matches.push({ from, to });
        resume = Math.max(resume, to);
      }
      if (++collected >= MATCH_BATCH || performance.now() - started >= SLICE_MS) {
        timer = setTimeout(step, 0);
        return;
      }
    }
    const result = matches;
    matches = [];
    cursor = null;
    complete(result);
  }

  timer = setTimeout(step, 0);
  return () => {
    cancelled = true;
    clearTimeout(timer);
    matches = [];
    cursor = null;
  };
}
