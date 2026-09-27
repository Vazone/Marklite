import { expect, test } from 'vitest';
import { treeRows, treeWindow } from './tree';
import type { DirectoryState, WorkspaceState } from './store';
import type { WorkspaceEntry } from '../../lib/platform/workspace';

function directory(entries: WorkspaceEntry[], next = false): DirectoryState {
  return { expanded: true, loaded: true, busy: false, revision: 1, generation: 1,
    entries, next: next ? { generation: 1, offset: entries.length } : null, issue: null, watchIssue: null };
}
function entry(path: string, kind: WorkspaceEntry['kind'] = 'markdown'): WorkspaceEntry {
  return { name: path.split('/').at(-1)!, relativePath: path, kind, resource: null, size: null, issue: null };
}
function state(nodes: Map<string, DirectoryState>): WorkspaceState {
  return { root: null, savedRoot: null, nodes, recent: [], restoreOnStart: true,
    supported: true, loading: false, issue: null, persistenceIssue: null };
}

test('flattened tree preserves sibling positions and inserts children only for expanded nodes', () => {
  const nodes = new Map([['', directory([entry('folder', 'directory'), entry('z.md')], true)],
    ['folder', directory([entry('folder/a.md')])]]);
  const rows = treeRows(state(nodes));
  expect(rows.map(row => [row.path, row.depth, row.position, row.count])).toEqual([
    ['folder', 1, 1, 3], ['folder/a.md', 2, 1, 1], ['z.md', 1, 2, 3], ['', 1, 3, 3]
  ]);
  expect(rows.at(-1)?.action).toBe('more');
  nodes.delete('folder');
  expect(treeRows(state(nodes))).toHaveLength(3);
});

test('100000 metadata rows produce bounded viewport slices at beginning, middle and end', () => {
  const rows = treeRows(state(new Map([['', directory(Array.from({ length: 100000 }, (_, i) => entry(`${i}.md`)))]])));
  for (const offset of [0, 1500000, 2999500]) {
    const range = treeWindow(rows.length, offset, 500, 30);
    expect(range.end - range.start).toBeLessThanOrEqual(30);
    expect(range.totalHeight).toBe(3000000);
    expect(rows.slice(range.start, range.end).length).toBeGreaterThan(0);
  }
});
