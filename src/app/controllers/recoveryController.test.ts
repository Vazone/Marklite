import { expect, test, vi } from 'vitest';
import { get } from 'svelte/store';
import { createDocumentStore } from '../stores/documentStore';
import { createRecoveryController } from './recoveryController';
import type { RecoverySnapshot } from '../../lib/platform/recovery';

function fixture() {
  const store = createDocumentStore();
  const api = {
    saveRecovery: vi.fn(async (s: RecoverySnapshot) => ({ id: s.id, revision: s.revision, checksum: `sha256:${'0'.repeat(64)}`, updatedAt: '2026-09-23T00:00:00Z' })),
    resolveRecovery: vi.fn(async () => {}),
    listRecovery: vi.fn(async () => ({ entries: [], issues: [] })),
    readRecovery: vi.fn()
  };
  let sequence = 0;
  const controller = createRecoveryController({ api, subscribe: store.subscribe,
    openCopy: s => store.openDocument({ path: null, resource: null, fileIdentity: null, contentVersion: null, title: s.title, content: s.content, isDirty: true, lastSavedAt: null, fileSize: null }),
    flushEditor: vi.fn(), onInventory: vi.fn(), onError: vi.fn(), createId: () => `copy-${++sequence}` });
  return { store, api, controller };
}

test('dirty untitled content is persisted; clean re-edit uses a fresh recovery identity', async () => {
  const { store, api, controller } = fixture();
  await controller.start(); store.newDocument();
  const id = get(store).activeTabId!;
  store.updateContent(id, 'unsaved'); await controller.flush();
  expect(api.saveRecovery.mock.calls[0][0]).toMatchObject({ id: 'copy-1', content: 'unsaved', resource: null });
  store.closeTab(id); await controller.flush();
  expect(api.resolveRecovery).toHaveBeenCalledOnce();
  store.newDocument(); store.updateContent(get(store).activeTabId!, 'other'); await controller.flush();
  expect(api.saveRecovery.mock.calls.at(-1)![0].id).toBe('copy-2');
  controller.dispose();
});

test('application teardown preserves acknowledged recovery copies', async () => {
  const { store, api, controller } = fixture();
  await controller.start(); store.newDocument(); store.updateContent(get(store).activeTabId!, 'recover after crash');
  await controller.flush(); controller.dispose();
  expect(api.resolveRecovery).not.toHaveBeenCalled();
});

test('explicit discard retires the exact acknowledged revision', async () => {
  const { store, api, controller } = fixture();
  await controller.start(); store.newDocument(); store.updateContent(get(store).activeTabId!, 'discard me');
  await controller.flush();
  await controller.discardOpenCopies();
  expect(api.resolveRecovery).toHaveBeenCalledWith(expect.objectContaining({ id: 'copy-1', revision: get(store).tabs.at(-1)!.contentRevision }));
  controller.dispose();
});

test('discard of queued content does not require new disk space', async () => {
  const { store, api, controller } = fixture();
  await controller.start(); store.newDocument(); store.updateContent(get(store).activeTabId!, 'discard me');
  await controller.discardOpenCopies();
  expect(api.saveRecovery).not.toHaveBeenCalled();
  expect(api.resolveRecovery).not.toHaveBeenCalled();
  controller.dispose();
});

test('failed discard resumes recovery with a new identity and keeps the editor dirty', async () => {
  const { store, api, controller } = fixture();
  await controller.start(); store.newDocument();
  const id = get(store).activeTabId!;
  store.updateContent(id, 'retain after failed exit'); await controller.flush();
  api.resolveRecovery.mockRejectedValueOnce(new Error('disk full'));
  await expect(controller.discardOpenCopies()).rejects.toThrow('disk full');
  expect(store.getTab(id)).toMatchObject({ isDirty: true, content: 'retain after failed exit' });
  await controller.flush();
  expect(api.saveRecovery.mock.calls.map(([s]) => s.id)).toEqual(['copy-1', 'copy-2']);
  controller.dispose();
});

test('failed persistence of a restored copy cannot retire the original recovery record', async () => {
  const { store, api, controller } = fixture();
  await controller.start();
  const snapshot: RecoverySnapshot = { id: 'old-copy', revision: 4, title: 'Recovered.md', content: 'keep this',
    resource: { kind: 'desktopFile', path: 'C:\\notes\\original.md' }, baseFileIdentity: 'old-file', baseContentVersion: 'old-version' };
  api.readRecovery.mockResolvedValue(snapshot);
  api.saveRecovery.mockRejectedValueOnce(new Error('disk full'));
  const entry = { receipt: { id: snapshot.id, revision: 4, checksum: `sha256:${'0'.repeat(64)}`, updatedAt: '2026-09-23T00:00:00Z' },
    title: snapshot.title, resource: snapshot.resource, contentBytes: 9, stale: false };
  await expect(controller.restore(entry)).rejects.toThrow('disk full');
  expect(api.resolveRecovery).not.toHaveBeenCalled();
  expect(get(store).tabs.at(-1)).toMatchObject({ path: null, content: 'keep this', isDirty: true });
  controller.dispose();
});
