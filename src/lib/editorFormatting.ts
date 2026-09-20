import { EditorSelection, type ChangeSpec, type EditorState, type SelectionRange, type TransactionSpec } from '@codemirror/state';

export type LineFormat = 'h1' | 'h2' | 'h3' | 'quote' | 'unorderedList' | 'orderedList' | 'taskList';

const linePrefixes: Record<LineFormat, string> = {
  h1: '# ',
  h2: '## ',
  h3: '### ',
  quote: '> ',
  unorderedList: '- ',
  orderedList: '1. ',
  taskList: '- [ ] '
};

const existingLinePrefix = /^(\s*)(- \[[ xX]\] +|#{1,6} +|> +|[-+*] +|\d+[.)] +)/;

export function formatLines(state: EditorState, action: LineFormat): TransactionSpec | null {
  const changes: ChangeSpec[] = [];
  const prefix = linePrefixes[action];
  let processedThrough = 0;
  for (const range of state.selection.ranges) {
    const last = range.to > range.from && state.doc.lineAt(range.to).from === range.to ? range.to - 1 : range.to;
    const firstLine = state.doc.lineAt(range.from).number;
    const lastLine = state.doc.lineAt(Math.max(range.from, last)).number;
    for (let number = Math.max(firstLine, processedThrough + 1); number <= lastLine; number++) {
      const line = state.doc.line(number);
      if (!line.text.trim()) continue;
      const match = existingLinePrefix.exec(line.text);
      const indent = match?.[1] ?? /^\s*/.exec(line.text)![0];
      const marker = match?.[2] ?? '';
      const body = line.text.slice(indent.length + marker.length);
      const alreadyFormatted = action === 'taskList'
        ? /^- \[[ xX]\] +$/.test(marker)
        : marker === prefix;
      const replacement = `${indent}${alreadyFormatted ? '' : prefix}${body}`;
      if (replacement !== line.text) changes.push({ from: line.from, to: line.to, insert: replacement });
    }
    processedThrough = Math.max(processedThrough, lastLine);
  }
  if (!changes.length) return null;
  const changeSet = state.changes(changes);
  const ranges = state.selection.ranges.map((range) => EditorSelection.range(
    changeSet.mapPos(range.anchor, 1),
    changeSet.mapPos(range.head, 1)
  ));
  return { changes: changeSet, selection: EditorSelection.create(ranges, state.selection.mainIndex) };
}

function longestBacktickRun(text: string): number {
  let longest = 0;
  for (const match of text.matchAll(/`+/g)) longest = Math.max(longest, match[0].length);
  return longest;
}

export function formatInlineCode(state: EditorState): TransactionSpec | null {
  return state.changeByRange((range) => {
    const selected = state.sliceDoc(range.from, range.to);
    if (selected.includes('\n')) return codeBlockRange(state, range);
    const body = selected || 'code';
    const beforeLine = state.doc.lineAt(range.from);
    const afterLine = state.doc.lineAt(range.to);
    const before = beforeLine.text.slice(0, range.from - beforeLine.from);
    const after = afterLine.text.slice(range.to - afterLine.from);
    const openingMatch = /(`+)( ?)$/.exec(before);
    const closingMatch = /^( ?)(`+)/.exec(after);
    const opening = openingMatch?.[1] ?? '';
    const closing = closingMatch?.[2] ?? '';
    const padding = openingMatch?.[2] ?? '';
    if (selected && opening && opening.length === closing.length
      && padding === (closingMatch?.[1] ?? '') && opening.length > longestBacktickRun(selected)) {
      const leftLength = opening.length + padding.length;
      const rightLength = closing.length + padding.length;
      return {
        changes: [
          { from: range.from - leftLength, to: range.from, insert: '' },
          { from: range.to, to: range.to + rightLength, insert: '' }
        ],
        range: EditorSelection.range(range.from - leftLength, range.to - leftLength)
      };
    }
    const delimiter = '`'.repeat(Math.max(1, longestBacktickRun(body) + 1));
    const pad = /^[` ]|[` ]$/.test(body) ? ' ' : '';
    const insert = `${delimiter}${pad}${body}${pad}${delimiter}`;
    const start = range.from + delimiter.length + pad.length;
    return {
      changes: { from: range.from, to: range.to, insert },
      range: EditorSelection.range(start, start + body.length)
    };
  });
}

export function formatCodeBlock(state: EditorState): TransactionSpec | null {
  let changed = false;
  const spec = state.changeByRange((range) => {
    const result = codeBlockRange(state, range);
    changed ||= Boolean(result.changes);
    return result;
  });
  return changed ? spec : null;
}

function codeBlockRange(state: EditorState, range: SelectionRange): { range: SelectionRange; changes?: ChangeSpec } {
  const first = state.doc.lineAt(range.from);
  const lastPosition = range.to > range.from && state.doc.lineAt(range.to).from === range.to
    ? range.to - 1 : range.to;
  const last = state.doc.lineAt(Math.max(range.from, lastPosition));
  if (range.from === first.from && range.to === last.to && first.number > 1 && last.number < state.doc.lines) {
    const opening = /^`{3,}$/.exec(state.doc.line(first.number - 1).text)?.[0];
    if (opening && state.doc.line(last.number + 1).text === opening) return { range };
  }

  const body = state.sliceDoc(range.from, range.to) || 'code';
  const before = state.sliceDoc(Math.max(0, range.from - 2), range.from);
  const after = state.sliceDoc(range.to, Math.min(state.doc.length, range.to + 2));
  const boundaryBefore = !range.from ? '' : before.endsWith('\n\n') ? '' : before.endsWith('\n') ? '\n' : '\n\n';
  const boundaryAfter = range.to === state.doc.length ? '' : after.startsWith('\n\n') ? '' : after.startsWith('\n') ? '\n' : '\n\n';
  const fence = '`'.repeat(Math.max(3, longestBacktickRun(body) + 1));
  const opening = `${boundaryBefore}${fence}\n`;
  const closing = `${body.endsWith('\n') ? '' : '\n'}${fence}${boundaryAfter}`;
  const insert = `${opening}${body}${closing}`;
  const start = range.from + opening.length;
  return {
    changes: { from: range.from, to: range.to, insert },
    range: EditorSelection.range(start, start + body.length)
  };
}
