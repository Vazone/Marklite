import { createDesktopStorage } from '../../lib/platform/resourceStorage';
import { get } from 'svelte/store';
import { expect, test, vi } from 'vitest';
import { createDocumentStore } from '../stores/documentStore';
import { createSaveController } from './saveController';
import { desktopFile } from '../../lib/platform/resources';

test.each([false, true])('recovery failure allows an explicit file save; dirty follows the file result (failure=%s)', async (fail) => {
  const store = createDocumentStore(); store.newDocument();
  const tab = get(store).tabs.at(-1)!; store.updateContent(tab.id, 'precious text');
  const toast = vi.fn();
  const write = vi.fn(async () => {
    if (fail) throw { code: 'FILE_WRITE_FAILED', message: 'No space on destination' };
    return { document: { path: 'C:\\notes\\saved.md', fileIdentity: 'saved-id', contentVersion: 'saved-version',
      title: 'saved.md', content: 'precious text', isDirty: false, lastSavedAt: null, fileSize: 13 }, auxiliaryError: null };
  });
  const controller = createSaveController({
    documentStore: store, storage: createDesktopStorage({ resolveFileVersion: async () => null, openMarkdownFile: vi.fn(), saveMarkdownFile: write }),
    activeTab: () => store.getTab(tab.id) ?? null, flushEditor: vi.fn(), loadTab: vi.fn(),
    pickSaveResource: vi.fn(), refreshRecentFiles: async () => true,
    errorMessage: String, toast, closeModals: vi.fn(), onConflict: vi.fn(),
    flushRecovery: async () => { throw new Error('Recovery quota full'); }
  });
  expect(await controller.save(tab.id, 'C:\\notes\\saved.md', false)).toBe(!fail);
  expect(write).toHaveBeenCalledOnce();
  expect(store.getTab(tab.id)).toMatchObject({ content: 'precious text', isDirty: fail });
  expect(toast).toHaveBeenCalledWith('Error: Recovery quota full', 'error');
});

test('external conflict blocks autosave, deduplicates failure and exposes explicit decisions', async () => {
  const store = createDocumentStore(); store.newDocument();
  const tab = get(store).tabs.at(-1)!;
  const toast = vi.fn(), onConflict = vi.fn();
  const controller = createSaveController({
    documentStore: store,
    storage: createDesktopStorage({ resolveFileVersion: async () => null, openMarkdownFile: vi.fn(),
      saveMarkdownFile: vi.fn(async () => { throw { code: 'FILE_CONTENT_CHANGED', message: 'Changed externally' }; }) }),
    activeTab: () => store.getTab(tab.id) ?? null, flushEditor: vi.fn(), loadTab: vi.fn(),
    pickSaveResource: vi.fn(), refreshRecentFiles: async () => true,
    errorMessage: String, toast, closeModals: vi.fn(), onConflict
  });
  expect(await controller.save(tab.id, 'test.md', false)).toBe(false);
  expect(await controller.save(tab.id, 'test.md', false)).toBe(false);
  expect(controller.isBlocked(tab.id)).toBe(true);
  expect(toast).toHaveBeenCalledTimes(1);
  expect(onConflict).toHaveBeenLastCalledWith({ tabId: tab.id, resource: desktopFile('test.md'), path: 'test.md', title: tab.title, busy: false });
  controller.closeConflict();
  expect(onConflict).toHaveBeenLastCalledWith(null);
  expect(controller.isBlocked(tab.id)).toBe(true);
  controller.forget(tab.id);
  expect(controller.isBlocked(tab.id)).toBe(false);
});

test('Android save keeps the content URI and clears dirty only after a verified write', async () => {
  const store = createDocumentStore();
  const resource = { kind: 'androidDocument' as const, uri: 'content://provider/document/note.md' };
  const tab = store.openDocument({ resource, path: null, fileIdentity: 'android:note',
    contentVersion: 'sha256:old', title: 'note.md', content: '# old', isDirty: false,
    lastSavedAt: null, fileSize: 5 });
  store.updateContent(tab.id, '# changed');
  const write = vi.fn(async () => ({ document: { resource, path: null, fileIdentity: 'android:note',
    contentVersion: 'sha256:new', title: 'note.md', content: '# changed', isDirty: false,
    lastSavedAt: null, fileSize: 9 }, auxiliaryError: null }));
  const refreshRecentFiles = vi.fn(async () => true);
  const controller = createSaveController({
    documentStore: store,
    storage: { read: vi.fn(), version: vi.fn(async () => ({ fileIdentity: 'android:note', contentVersion: 'sha256:old' })), write },
    activeTab: () => store.getTab(tab.id) ?? null, flushEditor: vi.fn(), loadTab: vi.fn(),
    pickSaveResource: vi.fn(), refreshRecentFiles,
    errorMessage: String, toast: vi.fn(), closeModals: vi.fn(), onConflict: vi.fn()
  });

  expect(await controller.saveResource(tab.id, resource, false)).toBe(true);
  expect(write).toHaveBeenCalledWith({ resource, content: '# changed',
    expectedFileIdentity: 'android:note', expectedContentVersion: 'sha256:old', overwriteContentConflict: false });
  expect(store.getTab(tab.id)).toMatchObject({ resource, path: null, isDirty: false });
  expect(refreshRecentFiles).toHaveBeenCalledOnce();
});
