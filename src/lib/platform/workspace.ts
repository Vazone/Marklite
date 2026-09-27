import type { AppError } from './contracts';
import { parseResourceRef, type ResourceRef } from './resources';

export type WorkspaceRoot = { rootId: string; resource: ResourceRef; name: string };
export type RecentWorkspace = { resource: ResourceRef; name: string };
export type WorkspacePreferences = { version: 2; root: ResourceRef | null; expanded: string[];
  restoreOnStart: boolean; recent: RecentWorkspace[] };
export type WorkspaceEntry = {
  name: string; relativePath: string; kind: 'directory' | 'markdown' | 'unavailable';
  resource: ResourceRef | null; size: number | null; issue: AppError | null;
};
export type PageCursor = { generation: number; offset: number };
export type WorkspacePage = {
  rootId: string; relativePath: string; generation: number; entries: WorkspaceEntry[];
  next: PageCursor | null; watchIssue: AppError | null;
};
export type DirectoryRevision = { relativePath: string; generation: number };
export type WorkspaceInvalidation = { rootId: string; directories: DirectoryRevision[]; overflow: boolean };

function invalid(): never { throw { code: 'INVALID_RESPONSE', message: 'Invalid workspace response.' } satisfies AppError; }
function object(value: unknown): Record<string, unknown> {
  return value && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : invalid();
}
function string(value: unknown): string { return typeof value === 'string' ? value : invalid(); }
function integer(value: unknown): number {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= 0 ? value : invalid();
}
function error(value: unknown): AppError | null {
  if (value === null) return null;
  const item = object(value);
  return { code: string(item.code), message: string(item.message) };
}

export function parseWorkspaceRoot(value: unknown): WorkspaceRoot {
  const item = object(value), resource = parseResourceRef(item.resource);
  if (resource.kind !== 'desktopDirectory' && resource.kind !== 'androidTree') return invalid();
  return { rootId: string(item.rootId), resource, name: string(item.name) };
}

export function parseWorkspacePreferences(value: unknown): WorkspacePreferences {
  const item = object(value);
  if (item.version !== 2 || !Array.isArray(item.expanded) || item.expanded.length > 256
    || typeof item.restoreOnStart !== 'boolean' || !Array.isArray(item.recent) || item.recent.length > 12) return invalid();
  const root = item.root === null ? null : parseResourceRef(item.root);
  if (root !== null && root.kind !== 'desktopDirectory' && root.kind !== 'androidTree') return invalid();
  const expanded = item.expanded.map(string);
  if ((!root && expanded.length > 0) || expanded.some(path => path.length > 16_384 || /[\\\0]/.test(path)
    || (path !== '' && path.split('/').some(part => part === '' || part === '.' || part === '..')))) return invalid();
  const recent = item.recent.map((value): RecentWorkspace => {
    const entry = object(value), resource = parseResourceRef(entry.resource), name = string(entry.name);
    if ((resource.kind !== 'desktopDirectory' && resource.kind !== 'androidTree')
      || !name.trim() || name.length > 512 || /[\u0000-\u001f\u007f]/.test(name)) return invalid();
    return { resource, name };
  });
  return { version: 2, root, expanded, restoreOnStart: item.restoreOnStart, recent };
}

export function parseDirectoryRevision(value: unknown): DirectoryRevision {
  const item = object(value);
  return { relativePath: string(item.relativePath), generation: integer(item.generation) };
}

export function parseWorkspacePage(value: unknown): WorkspacePage {
  const item = object(value), revision = parseDirectoryRevision(item);
  if (!Array.isArray(item.entries) || item.entries.length > 256) return invalid();
  const entries = item.entries.map((value): WorkspaceEntry => {
    const entry = object(value), resource = entry.resource === null ? null : parseResourceRef(entry.resource);
    const kind = entry.kind;
    if (kind !== 'directory' && kind !== 'markdown' && kind !== 'unavailable') return invalid();
    const issue = error(entry.issue);
    if ((kind === 'directory' && resource !== null && resource.kind !== 'desktopDirectory' && resource.kind !== 'androidTree')
      || (kind === 'markdown' && resource?.kind !== 'desktopFile' && resource?.kind !== 'androidDocument')
      || (kind === 'unavailable' && (resource !== null || issue === null))) return invalid();
    return { name: string(entry.name), relativePath: string(entry.relativePath), kind,
      resource, size: entry.size === null ? null : integer(entry.size), issue };
  });
  let next: PageCursor | null = null;
  if (item.next !== null) {
    const cursor = object(item.next);
    next = { generation: integer(cursor.generation), offset: integer(cursor.offset) };
    if (next.generation !== revision.generation) return invalid();
  }
  return { rootId: string(item.rootId), ...revision, entries, next, watchIssue: error(item.watchIssue) };
}

export function parseWorkspaceInvalidation(value: unknown): WorkspaceInvalidation {
  const item = object(value);
  if (!Array.isArray(item.directories) || item.directories.length > 256 || typeof item.overflow !== 'boolean') return invalid();
  return { rootId: string(item.rootId), directories: item.directories.map(parseDirectoryRevision), overflow: item.overflow };
}
