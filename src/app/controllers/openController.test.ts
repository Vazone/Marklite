import { get } from 'svelte/store';
import { expect, test, vi } from 'vitest';
import { createDocumentStore } from '../stores/documentStore';
import type { ResourceRef } from '../../lib/platform/resources';
import { createOpenController } from './openController';

test('Android picker opens distinct content URIs with the same display name', async () => {
  const store = createDocumentStore();
  const first = { kind: 'androidDocument' as const, uri: 'content://provider/a/note.md' };
  const second = { kind: 'androidDocument' as const, uri: 'content://provider/b/note.md' };
  const pickDocumentResource = vi.fn().mockResolvedValueOnce(first).mockResolvedValueOnce(second);
  const read = vi.fn(async (resource: ResourceRef) => {
    if (resource.kind !== 'androidDocument') throw new Error('Expected Android document');
    return { document: {
      resource, path: null, fileIdentity: `android:${resource.uri}`, contentVersion: 'sha256:one',
      title: 'note.md', content: '# Note', isDirty: false, lastSavedAt: null, fileSize: 6
    }, auxiliaryError: null };
  });
  const refreshRecentFiles = vi.fn(async () => true);
  const controller = createOpenController({ documentStore: store, storage: { read },
    flushEditor: vi.fn(), forget: vi.fn(), confirmAction: vi.fn(), pickDocumentResource,
    refreshRecentFiles, errorMessage: String, toast: vi.fn() });

  await controller.pick();
  await controller.pick();
  expect(get(store).tabs.map(tab => tab.resource)).toEqual([first, second]);
  expect(get(store).tabs.map(tab => tab.path)).toEqual([null, null]);
  expect(refreshRecentFiles).toHaveBeenCalledTimes(2);
});
