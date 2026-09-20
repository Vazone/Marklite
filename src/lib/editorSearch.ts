import type { ChangeSet, Text } from '@codemirror/state';
import { RegExpCursor } from '@codemirror/search';

export type EditorSearchMatch = {
  from: number;
  to: number;
};

function escapeSearchLiteral(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

export function findEditorSearchMatches(
  doc: Text,
  query: string,
  from = 0,
  to = doc.length
): EditorSearchMatch[] {
  const needle = query.trim();
  if (!needle) return [];

  const matches: EditorSearchMatch[] = [];
  const cursor = new RegExpCursor(doc, escapeSearchLiteral(needle), { ignoreCase: true }, from, to);
  while (!cursor.next().done) {
    matches.push({ from: cursor.value.from, to: cursor.value.to });
  }
  return matches;
}

export function updateEditorSearchMatches(
  doc: Text,
  query: string,
  previousMatches: EditorSearchMatch[],
  changes: ChangeSet
): EditorSearchMatch[] {
  if (changes.empty) return previousMatches;

  const rescanned: EditorSearchMatch[] = [];
  const context = Math.max(1, query.trim().length);
  changes.iterChangedRanges((_fromA, _toA, fromB, toB) => {
    const from = Math.max(doc.lineAt(fromB).from, fromB - context);
    const to = Math.min(doc.lineAt(toB).to, toB + context);
    const last = rescanned.at(-1);
    if (last && from <= last.to + 1) {
      last.to = Math.max(last.to, to);
    } else {
      rescanned.push({ from, to });
    }
  });

  const updated: EditorSearchMatch[] = [];
  let rangeIndex = 0;
  for (const match of previousMatches) {
    const mapped = {
      from: changes.mapPos(match.from, 1),
      to: changes.mapPos(match.to, -1)
    };
    while (rangeIndex < rescanned.length && rescanned[rangeIndex].to < mapped.from) {
      rangeIndex += 1;
    }
    const range = rescanned[rangeIndex];
    if (!range || mapped.from < range.from || mapped.to > range.to) {
      updated.push(mapped);
    }
  }

  for (const range of rescanned) {
    for (const match of findEditorSearchMatches(doc, query, range.from, range.to)) {
      updated.push(match);
    }
  }

  return updated.sort((left, right) => left.from - right.from || left.to - right.to);
}

export function editorSearchReplacementChange(
  doc: Text,
  matches: readonly EditorSearchMatch[],
  replacement: string
): { from: number; to: number; insert: string } | null {
  const first = matches[0];
  const last = matches.at(-1);
  if (!first || !last) return null;

  const content: string[] = [];
  let cursor = first.from;
  for (const match of matches) {
    content.push(doc.sliceString(cursor, match.from), replacement);
    cursor = match.to;
  }
  return { from: first.from, to: last.to, insert: content.join('') };
}

export function firstEditorSearchMatchAtOrAfter(
  matches: readonly EditorSearchMatch[],
  position: number
): number {
  let low = 0;
  let high = matches.length;
  while (low < high) {
    const middle = (low + high) >>> 1;
    if (matches[middle].from < position) {
      low = middle + 1;
    } else {
      high = middle;
    }
  }
  return low < matches.length ? low : matches.length ? 0 : -1;
}

export function editorSearchMatchAtPosition(
  matches: readonly EditorSearchMatch[],
  position: number
): number {
  let low = 0;
  let high = matches.length;
  while (low < high) {
    const middle = (low + high) >>> 1;
    if (matches[middle].to < position) {
      low = middle + 1;
    } else {
      high = middle;
    }
  }
  const match = matches[low];
  return match && match.from <= position && position <= match.to ? low : -1;
}

function firstMatchEndingAfter(matches: readonly EditorSearchMatch[], position: number): number {
  let low = 0;
  let high = matches.length;
  while (low < high) {
    const middle = (low + high) >>> 1;
    if (matches[middle].to <= position) {
      low = middle + 1;
    } else {
      high = middle;
    }
  }
  return low;
}

export function visibleEditorSearchMatches(
  matches: readonly EditorSearchMatch[],
  visibleRanges: readonly EditorSearchMatch[],
  activeIndex: number
): Array<{ index: number; match: EditorSearchMatch }> {
  const visible: Array<{ index: number; match: EditorSearchMatch }> = [];
  const active = activeIndex >= 0 && activeIndex < matches.length ? activeIndex : -1;
  let lastIndex = -1;

  const add = (index: number) => {
    if (index <= lastIndex) return;
    visible.push({ index, match: matches[index] });
    lastIndex = index;
  };

  for (const range of visibleRanges) {
    let index = Math.max(lastIndex + 1, firstMatchEndingAfter(matches, range.from));
    while (index < matches.length && matches[index].from < range.to) {
      if (active > lastIndex && active < index) add(active);
      add(index);
      index += 1;
    }
  }
  if (active > lastIndex) add(active);

  return visible;
}
