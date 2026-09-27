import { writable } from 'svelte/store';
import type { AppError } from '../../lib/platform/contracts';
import type { PlatformCapabilities } from '../../lib/platform/capabilities';
import type { ResourceRef } from '../../lib/platform/resources';
import { isSameResource } from '../../lib/platform/resources';
import type { WorkspaceClient } from '../../lib/platform/workspaceClient';
import { parseWorkspaceInvalidation, type PageCursor, type RecentWorkspace, type WorkspaceEntry, type WorkspaceRoot } from '../../lib/platform/workspace';
import { toAppError } from '../../lib/platform/contractValidation';
import { createPreferencesWriter } from './preferences';
import { mergeMetadata } from './metadata';

export type DirectoryState = {
  expanded: boolean; busy: boolean; loaded: boolean; revision: number; generation: number;
  entries: WorkspaceEntry[]; next: PageCursor | null; issue: AppError | null; watchIssue: AppError | null;
};
export type WorkspaceState = {
  root: WorkspaceRoot | null; savedRoot: ResourceRef | null; nodes: Map<string, DirectoryState>;
  recent: RecentWorkspace[]; restoreOnStart: boolean;
  supported: boolean | null; loading: boolean; issue: AppError | null; persistenceIssue: AppError | null;
  listenerIssue?: AppError;
};
type Dependencies = {
  client: WorkspaceClient;
  capabilities: () => Promise<PlatformCapabilities>;
  pickDirectory: () => Promise<ResourceRef | null>;
  listen: (receive: (value: unknown) => void) => Promise<() => void>;
  open: (resource: ResourceRef) => Promise<unknown>;
};
const emptyNode = (): DirectoryState => ({ expanded: true, busy: false, loaded: false, revision: 0, generation: 0, entries: [], next: null, issue: null, watchIssue: null });
const below = (path: string, parent: string) => parent === '' || path === parent || path.startsWith(parent + '/');

export function createWorkspaceStore(deps: Dependencies) {
  let state: WorkspaceState = { root: null, savedRoot: null, nodes: new Map(), recent: [], restoreOnStart: true,
    supported: null, loading: false, issue: null, persistenceIssue: null };
  const store = writable(state);
  let epoch = 0, sequence = 0, nodeRevision = 0, disposed = false, interaction = 0;
  let initialization: Promise<void> | null = null;
  let resolvePreferences!: () => void;
  const preferencesReady = new Promise<void>(resolve => { resolvePreferences = resolve; });
  let preferencesSettled = false;
  function settlePreferences() {
    if (preferencesSettled) return;
    preferencesSettled = true;
    resolvePreferences();
  }
  // A recreated path must never accept a response from its previous incarnation.
  const freshNode = (): DirectoryState => ({ ...emptyNode(), revision: ++nodeRevision });
  let unlisten: (() => void) | undefined;
  let queue = Promise.resolve();
  const pending = new Map<string, { rootId: string; path: string }>();
  const writer = createPreferencesWriter(value => deps.client.savePreferences(value), error => publish({ persistenceIssue: toAppError(error) }));

  function publish(change: Partial<WorkspaceState>) { state = { ...state, ...change }; if (!disposed) store.set(state); }
  function node(path: string, change: Partial<DirectoryState>) {
    const nodes = new Map(state.nodes);
    nodes.set(path, { ...(nodes.get(path) ?? freshNode()), ...change });
    publish({ nodes });
  }
  function persist() {
    return writer.write({ version: 2, root: state.root?.resource ?? state.savedRoot,
      expanded: state.root ? [...state.nodes].filter(([, value]) => value.expanded).map(([path]) => path).slice(0, 256) : [],
      restoreOnStart: state.restoreOnStart, recent: state.recent });
  }
  function current(requestEpoch: number, rootId: string) { return !disposed && epoch === requestEpoch && state.root?.rootId === rootId; }
  function cancel(parent = '', recursive = true) {
    for (const [id, job] of pending) if (recursive ? below(job.path, parent) : job.path === parent) {
      void deps.client.cancel(job.rootId, id).catch(error => {
        if (toAppError(error).code !== 'WORKSPACE_STALE') publish({ issue: toAppError(error) });
      });
    }
  }
  async function release(rootId: string) {
    try { await deps.client.unmount(rootId); }
    catch (error) { if (toAppError(error).code !== 'WORKSPACE_STALE') publish({ issue: toAppError(error) }); }
  }

  async function mount(resource: ResourceRef, remember = true) {
    const requestEpoch = ++epoch;
    const previousRoot = state.root?.rootId;
    cancel();
    publish({ root: null, savedRoot: resource, nodes: new Map(), loading: true, issue: null });
    try {
      const root = await deps.client.mount(resource);
      if (disposed || requestEpoch !== epoch) { await release(root.rootId); return; }
      const recent = [{ resource: root.resource, name: root.name },
        ...state.recent.filter(entry => !isSameResource(entry.resource, root.resource))].slice(0, 12);
      publish({ root, savedRoot: root.resource, recent, loading: false, nodes: new Map([['', freshNode()]]) });
      if (remember) void persist();
      await load('');
    } catch (error) {
      if (previousRoot) await release(previousRoot);
      if (!disposed && requestEpoch === epoch) publish({ loading: false, issue: toAppError(error) });
    }
  }

  function load(path: string, more = false): Promise<void> {
    const rootId = state.root?.rootId, existing = state.nodes.get(path);
    if (!rootId || !existing?.expanded || existing.busy || (more && !existing.next)) return Promise.resolve();
    const requestEpoch = epoch, revision = existing.revision, cursor = more ? existing.next : null;
    const requestId = `tree-${requestEpoch}-${++sequence}`;
    pending.set(requestId, { rootId, path });
    node(path, { busy: true, issue: null });
    const valid = () => current(requestEpoch, rootId) && state.nodes.get(path)?.revision === revision && state.nodes.get(path)?.expanded;
    queue = queue.then(async () => {
      try {
        if (!valid()) return;
        const page = await deps.client.list(rootId, path, cursor, requestId);
        if (!valid()) return;
        const value = state.nodes.get(path)!;
        if (page.rootId !== rootId || page.relativePath !== path || page.generation < value.generation) return;
        node(path, { busy: false, loaded: true, generation: page.generation,
          entries: mergeMetadata(state.nodes, path, page.entries, more), next: page.next, watchIssue: page.watchIssue });
      } catch (error) {
        if (valid()) node(path, { busy: false, issue: toAppError(error) });
      } finally {
        pending.delete(requestId);
        if (valid() && state.nodes.get(path)?.busy) node(path, { busy: false });
      }
    });
    return queue;
  }

  async function expand(path: string, manual = true) {
    if (manual) interaction++;
    if (!state.root) return;
    if (!state.nodes.has(path) && state.nodes.size >= 256) { publish({ issue: { code: 'WORKSPACE_EXPANSION_LIMIT', message: 'Too many expanded directories.' } }); return; }
    const value = state.nodes.get(path);
    node(path, { expanded: true });
    if (manual) void persist();
    if (!value?.loaded) await load(path);
  }

  async function findEntry(parent: string, path: string, requestEpoch: number) {
    while (!disposed && epoch === requestEpoch) {
      const value = state.nodes.get(parent);
      const entry = value?.entries.find(entry => entry.relativePath === path);
      if (entry) return entry;
      if (!value?.next || value.busy || value.issue) return null;
      await load(parent, true);
    }
    return null;
  }

  // Traverse and page only ancestors of the requested item, never unrelated subtrees.
  async function reveal(path: string, directory = false, manual = true): Promise<boolean> {
    if (manual) interaction++;
    const requestEpoch = epoch;
    if (!state.root || !path || path.split('/').some(part => !part || part === '.' || part === '..')) return false;
    let parent = '';
    const parts = path.split('/');
    for (let index = 0; index < parts.length; index++) {
      const target = parent ? `${parent}/${parts[index]}` : parts[index];
      const entry = await findEntry(parent, target, requestEpoch);
      if (!entry || disposed || epoch !== requestEpoch) return false;
      if (index < parts.length - 1 || directory) {
        if (entry.kind !== 'directory') return false;
        await expand(target, false);
        if (!state.nodes.get(target)?.loaded) return false;
      }
      parent = target;
    }
    if (manual) void persist();
    return true;
  }

  async function collapse(path: string) {
    interaction++;
    const rootId = state.root?.rootId;
    if (!rootId || path === '') return;
    cancel(path);
    const nodes = new Map(state.nodes);
    for (const key of nodes.keys()) if (below(key, path)) nodes.delete(key);
    publish({ nodes });
    void persist();
    try { await deps.client.collapse(rootId, path); } catch (error) { if (state.root?.rootId === rootId) publish({ issue: toAppError(error) }); }
  }

  async function refresh(path = '') {
    interaction++;
    const rootId = state.root?.rootId, requestEpoch = epoch;
    if (!rootId) return;
    cancel(path);
    const paths = [...state.nodes.keys()].filter(key => below(key, path));
    for (const key of paths) {
      node(key, freshNode());
    }
    try {
      for (const key of paths) {
        if (!current(requestEpoch, rootId)) return;
        const expected = state.nodes.get(key)?.revision;
        const revision = await deps.client.refresh(rootId, key);
        if (!current(requestEpoch, rootId)) return;
        if (state.nodes.get(key)?.revision !== expected) continue;
        node(key, { generation: revision.generation });
        await load(key);
      }
    } catch (error) { if (current(requestEpoch, rootId)) node(path, { busy: false, issue: toAppError(error) }); }
  }

  function invalidate(value: unknown) {
    try {
      const event = parseWorkspaceInvalidation(value);
      if (event.rootId !== state.root?.rootId) return;
      for (const revision of event.directories) {
        const value = state.nodes.get(revision.relativePath);
        if (!value || revision.generation <= value.generation) continue;
        cancel(revision.relativePath, false);
        node(revision.relativePath, { ...freshNode(), generation: revision.generation });
        void load(revision.relativePath);
      }
    } catch (error) { publish({ issue: toAppError(error) }); }
  }

  async function initialize() {
    const requestEpoch = epoch, beforeInteraction = interaction;
    try {
      const capabilities = await deps.capabilities();
      if (disposed) return;
      publish({ supported: capabilities.desktopFiles || capabilities.documentUris });
      if (!capabilities.desktopFiles && !capabilities.documentUris) { settlePreferences(); return; }
      try {
        const stop = await deps.listen(invalidate);
        if (disposed) { stop(); return; }
        unlisten = stop;
      } catch (error) {
        if (disposed) return;
        // Manual browsing/refresh remains useful when native notifications are unavailable.
        publish({ listenerIssue: toAppError(error) });
      }
      const preferences = await deps.client.loadPreferences();
      if (disposed) { settlePreferences(); return; }
      if (epoch !== requestEpoch || interaction !== beforeInteraction) {
        publish({ recent: [...state.recent, ...preferences.recent.filter(entry =>
          !state.recent.some(current => isSameResource(current.resource, entry.resource)))].slice(0, 12),
          restoreOnStart: preferences.restoreOnStart });
        settlePreferences();
        return;
      }
      publish({ recent: preferences.recent, restoreOnStart: preferences.restoreOnStart,
        savedRoot: preferences.root });
      settlePreferences();
      if (!preferences.restoreOnStart || !preferences.root) return;
      const restoredEpoch = epoch + 1;
      await mount(preferences.root, false);
      for (const path of preferences.expanded.filter(Boolean).sort((a, b) => a.split('/').length - b.split('/').length)) {
        if (disposed || epoch !== restoredEpoch || interaction !== beforeInteraction) break;
        await reveal(path, true, false);
      }
      if (!disposed && epoch === restoredEpoch && state.root) void persist();
    } catch (error) { if (!disposed && epoch === requestEpoch) publish({ issue: toAppError(error) }); }
    finally { settlePreferences(); }
  }

  function start() {
    initialization ??= initialize();
    return initialization;
  }

  async function choose() {
    interaction++;
    if (initialization) await preferencesReady;
    try { const resource = await deps.pickDirectory(); if (resource !== null) await mount(resource); }
    catch (error) { publish({ issue: toAppError(error) }); }
  }

  async function openRecent(resource: ResourceRef) {
    interaction++;
    if (!state.recent.some(entry => isSameResource(entry.resource, resource))) return;
    if (isSameResource(state.root?.resource, resource)) return;
    await mount(resource);
  }

  async function forgetRecent(resource: ResourceRef) {
    interaction++;
    if (isSameResource(state.root?.resource, resource)) await clear();
    publish({ recent: state.recent.filter(entry => !isSameResource(entry.resource, resource)),
      savedRoot: isSameResource(state.savedRoot, resource) ? null : state.savedRoot });
    void persist();
  }

  async function setRestoreOnStart(enabled: boolean) {
    interaction++;
    if (initialization) await preferencesReady;
    publish({ restoreOnStart: enabled });
    void persist();
  }

  async function open(path: string) {
    interaction++;
    const rootId = state.root?.rootId, requestEpoch = epoch;
    if (!rootId) return;
    try { const resource = await deps.client.resolve(rootId, path); if (current(requestEpoch, rootId)) await deps.open(resource); }
    catch (error) { if (current(requestEpoch, rootId)) publish({ issue: toAppError(error) }); }
  }

  async function clear() {
    interaction++; epoch++; cancel();
    const rootId = state.root?.rootId;
    publish({ root: null, savedRoot: null, nodes: new Map(), loading: false, issue: null });
    void persist();
    if (rootId) await release(rootId);
  }

  function dispose() {
    disposed = true; epoch++; cancel(); unlisten?.(); unlisten = undefined;
    if (state.root) void release(state.root.rootId);
  }

  return { subscribe: store.subscribe, start, choose, openRecent, forgetRecent, setRestoreOnStart,
    mount, expand, collapse, refresh, load, reveal, open, clear, dispose,
    retry: () => state.savedRoot ? mount(state.savedRoot) : Promise.resolve(), flush: writer.flush };
}

export type WorkspaceStore = ReturnType<typeof createWorkspaceStore>;
