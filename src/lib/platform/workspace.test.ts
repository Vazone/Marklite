import { expect, test } from 'vitest';
import fixture from '../../shared/workspace-contract-fixtures.json';
import { parseWorkspaceRoot, parseWorkspacePage, parseWorkspaceInvalidation, parseWorkspacePreferences } from './workspace';
import { createWorkspaceClient } from './workspaceClient';

test('workspace metadata and invalidations match shared IPC fixtures', () => {
  expect(parseWorkspaceRoot(fixture.root)).toEqual(fixture.root);
  expect(parseWorkspacePage(fixture.page)).toEqual(fixture.page);
  expect(parseWorkspaceInvalidation(fixture.invalidation)).toEqual(fixture.invalidation);
  expect(parseWorkspacePage({ ...fixture.page, watchIssue: fixture.failure }).watchIssue).toEqual(fixture.failure);
});

test('workspace preferences accept restorable locations and reject invalid expansion state', () => {
  const preferences = { version: 2, root: fixture.root.resource, expanded: ['', 'chapter'],
    restoreOnStart: true, recent: [{ resource: fixture.root.resource, name: 'notes' }] };
  expect(parseWorkspacePreferences(preferences)).toEqual(preferences);
  expect(() => parseWorkspacePreferences({ ...preferences, version: 3 })).toThrow();
  expect(() => parseWorkspacePreferences({ ...preferences, root: null })).toThrow();
  expect(() => parseWorkspacePreferences({ ...preferences, expanded: ['../outside'] })).toThrow();
});

test('workspace rejects wrong resource kinds, unbounded pages, and mismatched generations', () => {
  expect(() => parseWorkspaceRoot({ ...fixture.root, resource: { kind: 'desktopFile', path: '/notes.md' } })).toThrow();
  expect(() => parseWorkspacePage({ ...fixture.page, entries: Array(257).fill(fixture.page.entries[0]) })).toThrow();
  expect(() => parseWorkspacePage({ ...fixture.page, next: { generation: 9, offset: 3 } })).toThrow();
  expect(() => parseWorkspacePage({ ...fixture.page, generation: -1 })).toThrow();
  expect(() => parseWorkspacePage({ ...fixture.page, entries: [{ ...fixture.page.entries[1], kind: 'directory' }] })).toThrow();
  expect(() => parseWorkspaceInvalidation({ ...fixture.invalidation, overflow: 'yes' })).toThrow();
});

test('workspace client preserves cursor and root identity and refuses unavailable platform', async () => {
  const calls: unknown[] = [];
  const client = createWorkspaceClient({ invoke: async (command, args) => {
    calls.push({ command, args });
    if (command === 'mount_workspace') return fixture.root;
    return fixture.page;
  } }, async () => ({ platform: 'windows', desktopFiles: true, documentUris: false, exportFormats: [] }));
  const root = await client.mount({ kind: 'desktopDirectory', path: 'C:\\notes' });
  expect(root).toEqual(fixture.root);
  expect(await client.list(root.rootId, '', null, 'request-1', 32)).toEqual(fixture.page);
  expect(calls[1]).toEqual({ command: 'list_workspace', args: { rootId: root.rootId, relativePath: '', cursor: null, requestId: 'request-1', limit: 32 } });
  const mobile = createWorkspaceClient({ invoke: async () => { throw new Error('native command must not run'); } }, async () => ({ platform: 'android', desktopFiles: false, documentUris: false, exportFormats: [] }));
  await expect(mobile.mount({ kind: 'desktopDirectory', path: '/storage' })).rejects.toMatchObject({ code: 'CAPABILITY_UNAVAILABLE' });
});

test('Android workspace uses tree commands and opens a document URI from a nested directory', async () => {
  const rootResource = { kind: 'androidTree' as const, uri: 'content://provider/tree/root' };
  const fileResource = { kind: 'androidDocument' as const, uri: 'content://provider/tree/root/document/chapter%2Fa.md' };
  const commands: string[] = [];
  const client = createWorkspaceClient({ invoke: async (command) => {
    commands.push(command);
    if (command === 'mount_android_workspace') return { rootId: 'android-workspace-1', resource: rootResource, name: '笔记' };
    if (command === 'list_android_workspace') return { rootId: 'android-workspace-1', relativePath: '', generation: 1,
      entries: [{ name: 'chapter', relativePath: 'chapter', kind: 'directory', resource: null, size: null, issue: null }],
      next: null, watchIssue: { code: 'WORKSPACE_WATCH_UNAVAILABLE', message: 'Refresh manually' } };
    if (command === 'resolve_android_workspace_file') return fileResource;
    throw new Error(`Unexpected command: ${command}`);
  } }, async () => ({ platform: 'android', desktopFiles: false, documentUris: true, exportFormats: [] }));
  expect((await client.mount(rootResource)).resource).toEqual(rootResource);
  expect((await client.list('android-workspace-1', '', null, 'request-1')).entries[0].resource).toBeNull();
  expect(await client.resolve('android-workspace-1', 'chapter/a.md')).toEqual(fileResource);
  expect(commands).toEqual(['mount_android_workspace', 'list_android_workspace', 'resolve_android_workspace_file']);
});
