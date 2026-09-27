import { expect, test, vi } from 'vitest';
import { createDesktopStorage } from './resourceStorage';
import { desktopFile } from './resources';

test('desktop storage preserves the path and optimistic write conditions', async () => {
  const client = { openMarkdownFile: vi.fn(), resolveFileVersion: vi.fn(), saveMarkdownFile: vi.fn() };
  const storage = createDesktopStorage(client);
  const path = 'C:\\notes\\100% #计划.md';
  const resource = desktopFile(path);
  await storage.read(resource);
  await storage.version(resource, false);
  await storage.write({ resource, content: 'draft', expectedFileIdentity: 'id', expectedContentVersion: 'hash', overwriteContentConflict: false });
  expect(client.openMarkdownFile).toHaveBeenCalledWith(path);
  expect(client.resolveFileVersion).toHaveBeenCalledWith(path, false);
  expect(client.saveMarkdownFile).toHaveBeenCalledWith(path, 'draft', 'id', 'hash', false);
  const android = { kind: 'androidDocument' as const, uri: 'content://provider/document/a' };
  await expect(storage.read(android)).rejects.toMatchObject({ code: 'RESOURCE_UNSUPPORTED' });
  await expect(storage.version(android, true)).rejects.toMatchObject({ code: 'RESOURCE_UNSUPPORTED' });
  await expect(storage.write({ resource: android, content: '', expectedFileIdentity: null, expectedContentVersion: null, overwriteContentConflict: true })).rejects.toMatchObject({ code: 'RESOURCE_UNSUPPORTED' });
  expect(client.openMarkdownFile).toHaveBeenCalledTimes(1);
  expect(client.saveMarkdownFile).toHaveBeenCalledTimes(1);
});
