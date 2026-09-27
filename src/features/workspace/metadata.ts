import type { WorkspaceEntry } from '../../lib/platform/workspace';

const MAX_ENTRIES = 200_000;
const MAX_BYTES = 64 * 1024 * 1024;
type Size = { count: number; bytes: number };
const sizes = new WeakMap<WorkspaceEntry[], Size>();

// Account strings as UTF-16 plus a fixed object allowance; this is not a JS heap/RSS measurement.
function size(entries: WorkspaceEntry[]): Size {
  let result = sizes.get(entries);
  if (result) return result;
  let bytes = 0;
  for (const entry of entries) {
    const resource = entry.resource;
    const location = resource && ('path' in resource ? resource.path : resource.uri);
    bytes += 160 + 2 * (entry.name.length + entry.relativePath.length + (location?.length ?? 0)
      + (entry.issue?.message.length ?? 0) + (entry.issue?.code.length ?? 0));
  }
  result = { count: entries.length, bytes }; sizes.set(entries, result); return result;
}

export function mergeMetadata(nodes: ReadonlyMap<string, { entries: WorkspaceEntry[] }>, path: string,
  page: WorkspaceEntry[], append: boolean): WorkspaceEntry[] {
  const previous = append ? nodes.get(path)?.entries ?? [] : [];
  const before = size(previous), added = size(page);
  const combined = { count: before.count + added.count, bytes: before.bytes + added.bytes };
  let count = combined.count, bytes = combined.bytes;
  for (const [key, value] of nodes) if (key !== path) {
    const stored = size(value.entries); count += stored.count; bytes += stored.bytes;
  }
  if (count > MAX_ENTRIES || bytes > MAX_BYTES) throw {
    code: 'WORKSPACE_METADATA_LIMIT',
    message: 'The workspace metadata limit was reached. Collapse other folders before loading more.'
  };
  const entries = append ? [...previous, ...page] : page;
  sizes.set(entries, combined);
  return entries;
}
