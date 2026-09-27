import { get } from 'svelte/store';
import { expect, test, vi } from 'vitest';
import { createWorkspaceStore } from './store';
import type { WorkspaceClient } from '../../lib/platform/workspaceClient';
import type { WorkspacePage, WorkspacePreferences } from '../../lib/platform/workspace';
import type { ResourceRef } from '../../lib/platform/resources';

const rootRef: ResourceRef = { kind: 'desktopDirectory', path: 'C:\\notes' };
const page = (rootId: string, relativePath = '', name = 'a.md'): WorkspacePage => ({
  rootId, relativePath, generation: 1, entries: [{ name, relativePath: name, kind: 'markdown',
    resource: { kind: 'desktopFile', path: 'C:\\notes\\' + name }, size: 1, issue: null }], next: null, watchIssue: null
});
function setup(overrides: Partial<WorkspaceClient> = {}, listenerError = false) {
  let receive: (value: unknown) => void = () => {};
  let rootNumber = 0;
  const client: WorkspaceClient = {
    mount: vi.fn(async resource => ({ rootId: `root-${++rootNumber}`, resource, name: 'notes' })),
    unmount: vi.fn(async () => {}), list: vi.fn(async (id, path) => page(id, path)),
    cancel: vi.fn(async () => {}), collapse: vi.fn(async () => {}),
    refresh: vi.fn(async (_, relativePath) => ({ relativePath, generation: 1 })),
    resolve: vi.fn(async (_, path) => ({ kind: 'desktopFile' as const, path: 'C:\\notes\\' + path })),
    loadPreferences: vi.fn(async () => ({ version: 2 as const, root: null, expanded: [], restoreOnStart: true, recent: [] })),
    savePreferences: vi.fn(async value => value), ...overrides
  };
  const open = vi.fn(async () => {}), stop = vi.fn();
  const store = createWorkspaceStore({ client, open, capabilities: async () => ({ platform: 'windows', desktopFiles: true, documentUris: false, exportFormats: [] }),
    pickDirectory: async () => ({ kind: 'desktopDirectory', path: 'C:\\notes' }), listen: async callback => {
      if (listenerError) throw { code: 'WATCH_FAILED', message: 'No notifications' };
      receive = callback; return stop;
    } });
  return { store, client, open, stop, receive: (value: unknown) => receive(value) };
}
async function settle() { for (let i = 0; i < 12; i++) await Promise.resolve(); }

test('notification subscription failure remains visible without preventing root restore or manual refresh', async () => {
  const { store } = setup({ loadPreferences: async () => ({ version: 2, root: rootRef, expanded: [], restoreOnStart: true,
    recent: [{ resource: rootRef, name: 'notes' }] }) }, true);
  await store.start();
  expect(get(store).root?.rootId).toBe('root-1');
  expect(get(store).listenerIssue?.code).toBe('WATCH_FAILED');
  await store.refresh();
  expect(get(store).nodes.get('')?.loaded).toBe(true);
  store.dispose();
});

test('recent folders survive close and restore independently from document tabs', async () => {
  const other: ResourceRef = { kind: 'desktopDirectory', path: 'C:\\other' };
  const initial: WorkspacePreferences = { version: 2, root: rootRef, expanded: [], restoreOnStart: false,
    recent: [{ resource: rootRef, name: 'notes' }, { resource: other, name: 'other' }] };
  const { store, client } = setup({ loadPreferences: async () => initial });
  await store.start();
  expect(get(store).root).toBeNull();
  expect(get(store).recent).toHaveLength(2);
  await store.openRecent(other);
  expect(get(store).root?.resource).toEqual(other);
  expect(get(store).recent[0].resource).toEqual(other);
  await store.clear();
  expect(get(store).root).toBeNull();
  expect(get(store).recent).toHaveLength(2);
  store.setRestoreOnStart(true);
  await store.forgetRecent(other);
  await store.flush();
  expect(get(store).recent).toEqual([{ resource: rootRef, name: 'notes' }]);
  expect(client.savePreferences).toHaveBeenLastCalledWith({ version: 2, root: null, expanded: [],
    restoreOnStart: true, recent: [{ resource: rootRef, name: 'notes' }] });
  store.dispose();
});

test('choosing a folder while startup reads history keeps older recent folders', async () => {
  const other: ResourceRef = { kind: 'desktopDirectory', path: 'C:\\older' };
  let finish!: (value: WorkspacePreferences) => void;
  const pending = new Promise<WorkspacePreferences>(resolve => { finish = resolve; });
  const { store } = setup({ loadPreferences: () => pending });
  const startup = store.start(); await settle();
  const choose = store.choose();
  finish({ version: 2, root: other, expanded: [], restoreOnStart: true,
    recent: [{ resource: other, name: 'older' }] });
  await Promise.all([startup, choose]);
  expect(get(store).root?.resource).toEqual(rootRef);
  expect(get(store).recent.map(entry => entry.name)).toEqual(['notes', 'older']);
  store.dispose();
});

test('failed restoration retains the saved location and recent choice for retry', async () => {
  const { store, client } = setup({ loadPreferences: async () => ({ version: 2, root: rootRef, expanded: [''],
    restoreOnStart: true, recent: [{ resource: rootRef, name: 'notes' }] }),
    mount: vi.fn().mockRejectedValueOnce({ code: 'WORKSPACE_NOT_FOUND', message: 'gone' })
      .mockResolvedValueOnce({ rootId: 'recovered', resource: rootRef, name: 'notes' }) });
  await store.start();
  expect(get(store).root).toBeNull();
  expect(get(store).savedRoot).toEqual(rootRef);
  expect(get(store).recent).toHaveLength(1);
  expect(client.savePreferences).not.toHaveBeenCalled();
  await store.retry();
  expect(get(store).root?.rootId).toBe('recovered');
  store.dispose();
});

test('a root chosen while restoration is mounting does not receive the old expanded paths', async () => {
  let finish!: (root: { rootId: string; resource: ResourceRef; name: string }) => void;
  const lateRoot = new Promise<{ rootId: string; resource: ResourceRef; name: string }>(resolve => { finish = resolve; });
  const { store, client } = setup({ loadPreferences: async () => ({ version: 2, root: rootRef, expanded: ['child'],
    restoreOnStart: true, recent: [{ resource: rootRef, name: 'notes' }] }),
    mount: vi.fn().mockImplementationOnce(() => lateRoot).mockResolvedValue({ rootId: 'new-root', resource: rootRef, name: 'New' }) });
  const restore = store.start(); await settle();
  await store.choose();
  finish({ rootId: 'old-root', resource: rootRef, name: 'Old' });
  await restore;
  expect(get(store).root?.rootId).toBe('new-root');
  expect(client.list).toHaveBeenCalledTimes(1);
  expect(get(store).nodes.has('child')).toBe(false);
  store.dispose();
});

test('collapse and reexpand reject a late page from the removed node', async () => {
  let release!: (value: WorkspacePage) => void;
  const oldPage = new Promise<WorkspacePage>(resolve => { release = resolve; });
  let childCalls = 0;
  const { store } = setup({ list: vi.fn(async (id, path) => {
    if (path === 'child' && ++childCalls === 1) return oldPage;
    return page(id, path, 'new.md');
  }) });
  await store.mount(rootRef);
  const first = store.expand('child'); await settle();
  const firstRevision = get(store).nodes.get('child')!.revision;
  await store.collapse('child');
  const second = store.expand('child');
  expect(get(store).nodes.get('child')!.revision).not.toBe(firstRevision);
  const observed: string[] = [];
  const stop = store.subscribe(state => {
    observed.push(...(state.nodes.get('child')?.entries.map(entry => entry.name) ?? []));
  });
  release(page('root-1', 'child', 'old.md'));
  await Promise.all([first, second]);
  expect(observed).not.toContain('old.md');
  expect(get(store).nodes.get('child')?.entries[0].name).toBe('new.md');
  stop(); store.dispose();
});

test('restoring a directory beyond the first page loads its ancestors without opening documents', async () => {
  const { store, client, open } = setup({
    loadPreferences: async () => ({ version: 2, root: rootRef, expanded: ['late/nested'],
      restoreOnStart: true, recent: [{ resource: rootRef, name: 'notes' }] }),
    list: vi.fn(async (id, path, cursor): Promise<WorkspacePage> => {
      if (!path && !cursor) return { ...page(id), next: { generation: 1, offset: 1 } };
      const child = !path ? 'late' : path === 'late' ? 'late/nested' : null;
      return { ...page(id, path), entries: child ? [{ name: child.split('/').at(-1)!, relativePath: child,
        kind: 'directory', resource: null, size: null, issue: null }] : [], next: null };
    })
  });
  await store.start();
  expect([...get(store).nodes.keys()]).toEqual(['', 'late', 'late/nested']);
  expect(client.list).toHaveBeenCalledTimes(4);
  expect(open).not.toHaveBeenCalled();
  expect(await store.reveal('missing/file.md')).toBe(false);
  store.dispose();
});

test('workspace mount and refresh enumerate metadata; only explicit open delegates to document use case', async () => {
  const { store, client, open } = setup();
  await store.start(); await store.choose();
  expect(get(store).root?.rootId).toBe('root-1');
  expect(get(store).nodes.get('')?.entries[0].name).toBe('a.md');
  await store.refresh(); expect(open).not.toHaveBeenCalled();
  await store.open('a.md'); expect(open).toHaveBeenCalledWith({ kind: 'desktopFile', path: 'C:\\notes\\a.md' });
  await store.flush(); expect(client.savePreferences).toHaveBeenCalledWith({ version: 2, root: rootRef, expanded: [''],
    restoreOnStart: true, recent: [{ resource: rootRef, name: 'notes' }] });
  store.dispose();
});

test('a late page from the previous root cannot mix into the new tree', async () => {
  let release!: (page: WorkspacePage) => void;
  const firstPage = new Promise<WorkspacePage>(resolve => { release = resolve; });
  const { store, client } = setup({ list: vi.fn(async (id, path) => id === 'root-1' ? firstPage : page(id, path, 'new.md')) });
  const first = store.mount(rootRef); await settle();
  const second = store.mount({ kind: 'desktopDirectory', path: 'C:\\other' }); await settle();
  release(page('root-1', '', 'old.md'));
  await Promise.all([first, second]);
  expect(client.cancel).toHaveBeenCalled();
  expect(get(store).root?.rootId).toBe('root-2');
  expect(get(store).nodes.get('')?.entries.map(entry => entry.name)).toEqual(['new.md']);
  store.dispose();
});

test('restoration retains inaccessible root for retry and a failed replacement releases previous watches', async () => {
  const saved: WorkspacePreferences = { version: 2, root: rootRef, expanded: [''], restoreOnStart: true,
    recent: [{ resource: rootRef, name: 'notes' }] };
  const { store, client } = setup({ loadPreferences: async () => saved });
  await store.start();
  expect(get(store).root?.rootId).toBe('root-1');
  vi.mocked(client.mount).mockRejectedValueOnce({ code: 'WORKSPACE_PERMISSION_DENIED', message: 'denied' });
  await store.mount({ kind: 'desktopDirectory', path: 'C:\\denied' });
  expect(get(store).issue?.code).toBe('WORKSPACE_PERMISSION_DENIED');
  expect(get(store).savedRoot).toEqual({ kind: 'desktopDirectory', path: 'C:\\denied' });
  expect(client.unmount).toHaveBeenCalledWith('root-1');
  await store.retry(); expect(get(store).root?.rootId).toBe('root-2');
  store.dispose();
});

test('late invalidation for an old root is ignored and live invalidation reloads once', async () => {
  let generation = 1;
  const { store, client, receive, stop } = setup({ list: async (id, path) => ({ ...page(id, path), generation }) });
  await store.start(); await store.mount(rootRef);
  receive({ rootId: 'old-root', directories: [{ relativePath: '', generation: 2 }], overflow: true });
  expect(get(store).nodes.get('')?.generation).toBe(1);
  generation = 2;
  receive({ rootId: 'root-1', directories: [{ relativePath: '', generation: 2 }], overflow: true });
  await settle();
  expect(get(store).nodes.get('')?.generation).toBe(2);
  expect(get(store).nodes.get('')?.busy).toBe(false);
  store.dispose(); expect(stop).toHaveBeenCalledOnce(); expect(client.unmount).toHaveBeenCalledWith('root-1');
});
