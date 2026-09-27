interface Dollar {
  at: number;
  context: number;
  open: boolean;
  close: boolean;
}

const space = (char: string | undefined) => char === undefined || /[\t\n\r\f ]/.test(char);

/** Opaque source ranges, not a TeX parser. Mirrors the product parser's dollar
 * pairing: matching brace context, one or two dollars, and flanking whitespace.
 * Index each inline context once so unmatched dollars cannot cause rescanning. */
export function markdownMathRanges(text: string): Map<number, number> {
  const dollars: Dollar[] = [];
  const contexts: number[] = [];
  let nextContext = 0;
  for (let at = 0; at < text.length; at++) {
    const char = text[at];
    if (char === '\\' && /[!-/:-@[-`{-~]/.test(text[at + 1] ?? '')) { at++; continue; }
    if (char === '{' && contexts.length) contexts.push(++nextContext);
    if (char === '}' && contexts.length) {
      if (contexts.length === 1) contexts[0] = ++nextContext;
      else contexts.pop();
    }
    if (char === '$') {
      if (!contexts.length) contexts.push(++nextContext);
      dollars.push({ at, context: contexts[contexts.length - 1],
        open: !space(text[at + 1]), close: at > 0 && !space(text[at - 1]) });
    }
  }
  const next = new Map<number, number>();
  const following: Array<number | undefined> = [];
  for (let i = dollars.length - 1; i >= 0; i--) {
    following[i] = next.get(dollars[i].context);
    next.set(dollars[i].context, i);
  }
  const ranges = new Map<number, number>();
  for (let i = 0; i < dollars.length; i++) {
    const opening = dollars[i];
    if (!opening.open) continue;
    const display = dollars[i + 1]?.at === opening.at + 1;
    const end = following[i + (display ? 1 : 0)];
    if (end === undefined) continue;
    const closing = dollars[end];
    const displayEnd = dollars[end + 1]?.at === closing.at + 1;
    if (display ? displayEnd : closing.close) {
      const width = display && displayEnd ? 2 : 1;
      if (closing.at > opening.at + width) ranges.set(opening.at, closing.at + width);
    }
  }
  return ranges;
}
