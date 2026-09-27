import type { WorkspaceEntry } from '../../lib/platform/workspace';
import type { WorkspaceState } from './store';

export type TreeRow = {
  key: string; path: string; parent: string; depth: number;
  position: number; count: number; entry: WorkspaceEntry | null;
  action: 'entry' | 'more' | 'retry' | 'loading';
};

/** Flatten metadata only. File bodies remain owned by the document open use case. */
export function treeRows(state: WorkspaceState): TreeRow[] {
  const rows: TreeRow[] = [];
  const walk = (parent: string, depth: number) => {
    const node = state.nodes.get(parent);
    if (!node?.expanded) return;
    const action = node.issue ? 'retry' : node.busy ? 'loading' : node.next ? 'more' : null;
    const count = node.entries.length + (action ? 1 : 0);
    node.entries.forEach((entry, index) => {
      rows.push({ key: `entry:${entry.relativePath}`, path: entry.relativePath, parent, depth,
        position: index + 1, count, entry, action: 'entry' });
      if (entry.kind === 'directory') walk(entry.relativePath, depth + 1);
    });
    if (action) rows.push({ key: `status:${parent}`, path: parent, parent, depth,
      position: count, count, entry: null, action });
  };
  walk('', 1);
  return rows;
}

export function treeWindow(length: number, scrollTop: number, height: number, rowHeight: number, overscan = 6) {
  const start = Math.max(0, Math.min(length, Math.floor(Math.max(0, scrollTop) / rowHeight) - overscan));
  const end = Math.min(length, Math.max(start, Math.ceil((Math.max(0, scrollTop) + Math.max(0, height)) / rowHeight) + overscan));
  return { start, end, totalHeight: length * rowHeight };
}
