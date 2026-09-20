import { ChangeSet, EditorState } from '@codemirror/state';
import { describe, expect, test, vi } from 'vitest';
import {
  editorSearchMatchAtPosition,
  editorSearchReplacementChange,
  findEditorSearchMatches,
  firstEditorSearchMatchAtOrAfter,
  updateEditorSearchMatches,
  visibleEditorSearchMatches
} from './editorSearch';

describe('editor search state', () => {
  test('keeps source offsets correct when Unicode case folding expands a character', () => {
    const state = EditorState.create({ doc: 'İx and X' });

    expect(findEditorSearchMatches(state.doc, 'x')).toEqual([
      { from: 1, to: 2 },
      { from: 7, to: 8 }
    ]);
  });

  test('treats regexp punctuation literally and returns non-overlapping end matches', () => {
    const state = EditorState.create({ doc: 'a.b A.B axb aaa [$] end' });
    const toString = vi.spyOn(state.doc, 'toString');

    expect(findEditorSearchMatches(state.doc, 'a.b')).toEqual([
      { from: 0, to: 3 },
      { from: 4, to: 7 }
    ]);
    expect(findEditorSearchMatches(state.doc, 'aa')).toEqual([{ from: 12, to: 14 }]);
    expect(findEditorSearchMatches(state.doc, '[$]')).toEqual([{ from: 16, to: 19 }]);
    expect(findEditorSearchMatches(state.doc, 'end')).toEqual([{ from: 20, to: 23 }]);
    expect(findEditorSearchMatches(state.doc, '   ')).toEqual([]);
    expect(toString).not.toHaveBeenCalled();
  });

  test('incremental edits match a full reference rescan across changed lines', () => {
    const initial = EditorState.create({
      doc: 'needle first\nstable needle\nneedle last\nplain'
    });
    const matches = findEditorSearchMatches(initial.doc, 'needle');
    const transaction = initial.update({
      changes: [
        { from: 0, to: 6, insert: 'gone' },
        { from: 39, to: 39, insert: '\nneedle end' }
      ]
    });

    expect(updateEditorSearchMatches(transaction.state.doc, 'needle', matches, transaction.changes)).toEqual(
      findEditorSearchMatches(transaction.state.doc, 'needle')
    );
  });

  test('drops a deleted match when the changed line becomes empty', () => {
    const initial = EditorState.create({ doc: 'needle' });
    const matches = findEditorSearchMatches(initial.doc, 'needle');
    const transaction = initial.update({ changes: { from: 0, to: initial.doc.length, insert: '' } });

    expect(updateEditorSearchMatches(transaction.state.doc, 'needle', matches, transaction.changes)).toEqual([]);
  });

  test('rescans query-sized context when an edit joins or splits a match', () => {
    const split = EditorState.create({ doc: 'need\nle' });
    const joined = split.update({ changes: { from: 4, to: 5, insert: '' } });
    expect(updateEditorSearchMatches(joined.state.doc, 'needle', [], joined.changes)).toEqual([
      { from: 0, to: 6 }
    ]);

    const complete = EditorState.create({ doc: 'needle' });
    const broken = complete.update({ changes: { from: 4, insert: '\n' } });
    expect(
      updateEditorSearchMatches(
        broken.state.doc,
        'needle',
        findEditorSearchMatches(complete.doc, 'needle'),
        broken.changes
      )
    ).toEqual([]);
  });

  test('updates 150000 matches without depending on the JavaScript argument limit', () => {
    const initial = EditorState.create({ doc: 'a'.repeat(150_000) });
    const matches = findEditorSearchMatches(initial.doc, 'a');
    const transaction = initial.update({ changes: { from: initial.doc.length, insert: 'a' } });

    const updated = updateEditorSearchMatches(transaction.state.doc, 'a', matches, transaction.changes);

    expect(updated).toHaveLength(150_001);
    expect(updated[0]).toEqual({ from: 0, to: 1 });
    expect(updated.at(-1)).toEqual({ from: 150_000, to: 150_001 });

    const replacement = editorSearchReplacementChange(initial.doc, matches, 'b');
    expect(replacement).not.toBeNull();
    const replacements = ChangeSet.of(replacement!, initial.doc.length);
    const replaced = replacements.apply(initial.doc);
    expect(replaced.length).toBe(initial.doc.length);
    expect(updateEditorSearchMatches(replaced, 'a', matches, replacements)).toEqual([]);
    expect(replacements.invert(initial.doc).apply(replaced).toString()).toBe(initial.doc.toString());
  });

  test('builds one replace-all change set whose inverse restores the source', () => {
    const initial = EditorState.create({ doc: 'a.a A.A a-a' });
    const matches = findEditorSearchMatches(initial.doc, 'a.a');
    const replacement = editorSearchReplacementChange(initial.doc, matches, 'done');
    expect(replacement).not.toBeNull();
    const changes = ChangeSet.of(replacement!, initial.doc.length);
    const replaced = changes.apply(initial.doc);

    expect(replaced.toString()).toBe('done done a-a');
    expect(changes.invert(initial.doc).apply(replaced).toString()).toBe(initial.doc.toString());
  });

  test('uses binary lookup for active and next-match positions', () => {
    const matches = Array.from({ length: 150_000 }, (_, index) => ({
      from: index * 2,
      to: index * 2 + 1
    }));

    expect(editorSearchMatchAtPosition(matches, 200_001)).toBe(100_000);
    expect(editorSearchMatchAtPosition(matches, 200_001.5)).toBe(-1);
    expect(firstEditorSearchMatchAtOrAfter(matches, 200_001)).toBe(100_001);
    expect(firstEditorSearchMatchAtOrAfter(matches, 400_000)).toBe(0);
    expect(firstEditorSearchMatchAtOrAfter([], 0)).toBe(-1);
  });

  test('builds decorations only for visible matches plus the active off-screen match', () => {
    const matches = Array.from({ length: 1_000 }, (_, index) => ({
      from: index * 10,
      to: index * 10 + 4
    }));

    const visible = visibleEditorSearchMatches(matches, [{ from: 100, to: 200 }], 900);

    expect(visible.map(({ index }) => index)).toEqual([
      ...Array.from({ length: 10 }, (_, index) => index + 10),
      900
    ]);
  });

  test('locates visible matches without scanning the complete result set', () => {
    const matches = Array.from({ length: 150_000 }, (_, index) => ({
      from: index * 2,
      to: index * 2 + 1
    }));
    let reads = 0;
    const observed = new Proxy(matches, {
      get(target, property, receiver) {
        if (typeof property === 'string' && /^\d+$/.test(property)) {
          reads += 1;
          if (reads > 100) throw new Error('scanned the complete result set');
        }
        return Reflect.get(target, property, receiver);
      }
    });

    expect(visibleEditorSearchMatches(observed, [{ from: 200_000, to: 200_020 }], 140_000)).toEqual([
      ...Array.from({ length: 10 }, (_, offset) => ({
        index: 100_000 + offset,
        match: { from: 200_000 + offset * 2, to: 200_001 + offset * 2 }
      })),
      { index: 140_000, match: { from: 280_000, to: 280_001 } }
    ]);
    expect(reads).toBeLessThanOrEqual(100);
  });
});
