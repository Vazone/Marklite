import { describe, expect, it, vi } from 'vitest';
import { createAndroidDocuments } from './androidDocuments';
import { createResourceStorage } from './resourceStorage';
import type { TauriClient } from './tauriClient';
import type { DocumentOperationDto } from './contracts';

const uri = 'content://provider/document/%E4%B8%AD%E6%96%87.md';
const resource = { kind: 'androidDocument' as const, uri };
const operation: DocumentOperationDto = {
  document: { resource, path: null, fileIdentity: 'android:1', contentVersion: 'sha256:1',
    title: '中文.md', content: '# 中文', isDirty: false, lastSavedAt: null, fileSize: 8 },
  auxiliaryError: null
};

describe('Android document boundary', () => {
  it('loads and removes typed recent document resources', async () => {
    const recent = [{ resource, title: '中文.md', lastOpenedAt: '2026-09-25T00:00:00Z' }];
    const invoke = vi.fn(async () => recent);
    const android = createAndroidDocuments(invoke);
    expect(await android.getRecentDocuments()).toEqual(recent);
    expect(await android.removeRecentDocument(resource)).toEqual(recent);
    expect(invoke).toHaveBeenCalledWith('remove_android_recent_document', { resource });
    await expect(createAndroidDocuments(async () => [{ ...recent[0], resource: { kind: 'desktopFile', path: 'C:\\a.md' } }])
      .getRecentDocuments()).rejects.toMatchObject({ code: 'INVALID_RESPONSE' });
  });
  it('keeps content URI typed through picker, read and write', async () => {
    const invoke = vi.fn(async (command: string): Promise<unknown> => {
      if (command === 'pick_android_document') return resource;
      if (command === 'open_android_document' || command === 'save_android_document') return operation;
      if (command === 'android_document_version') return { fileIdentity: 'android:1', contentVersion: 'sha256:1' };
      return null;
    });
    const android = createAndroidDocuments(invoke);
    const desktop = { openMarkdownFile: vi.fn(), resolveFileVersion: vi.fn(), saveMarkdownFile: vi.fn() };
    const storage = createResourceStorage(desktop as unknown as Pick<TauriClient,
      'openMarkdownFile' | 'resolveFileVersion' | 'saveMarkdownFile'>, android);
    expect(await android.pickDocument()).toEqual(resource);
    expect(await storage.read(resource)).toEqual(operation);
    expect(await storage.version(resource, false)).toEqual({ fileIdentity: 'android:1', contentVersion: 'sha256:1' });
    expect(await storage.write({ resource, content: '# 中文', expectedFileIdentity: 'android:1',
      expectedContentVersion: 'sha256:1', overwriteContentConflict: false })).toEqual(operation);
    expect(desktop.openMarkdownFile).not.toHaveBeenCalled();
    expect(invoke).toHaveBeenCalledWith('open_android_document', { resource });
  });

  it('rejects an Android picker returning a desktop path', async () => {
    const android = createAndroidDocuments(async () => ({ kind: 'desktopFile', path: 'C:\\x.md' }));
    await expect(android.pickDocument()).rejects.toMatchObject({ code: 'INVALID_RESPONSE' });
  });

  it('reads a deferred document title from metadata without opening its content', async () => {
    const invoke = vi.fn(async () => '中文.md');
    const android = createAndroidDocuments(invoke);
    expect(await android.documentName(resource)).toBe('中文.md');
    expect(invoke).toHaveBeenCalledExactlyOnceWith('android_document_name', { resource });
    await expect(createAndroidDocuments(async () => '../wrong.md').documentName(resource))
      .rejects.toMatchObject({ code: 'INVALID_RESPONSE' });
    await expect(createAndroidDocuments(async () => 'wrong.md').documentName({
      kind: 'desktopFile', path: 'C:\\wrong.md'
    })).rejects.toMatchObject({ code: 'RESOURCE_UNSUPPORTED' });
  });

  it('passes the SVG format and name to SAF and rejects an untyped export target', async () => {
    const exportUri = 'content://provider/document/map.svg';
    const invoke = vi.fn(async () => ({ kind: 'androidDocument', uri: exportUri }));
    const android = createAndroidDocuments(invoke);
    expect(await android.createExportDocument('svg', 'map.svg')).toEqual({ kind: 'androidDocument', uri: exportUri });
    expect(invoke).toHaveBeenCalledWith('pick_android_export_document', { format: 'svg', title: 'map.svg' });
    const invalid = createAndroidDocuments(async () => ({ kind: 'desktopFile', path: 'C:\\map.svg' }));
    await expect(invalid.createExportDocument('svg', 'map.svg')).rejects.toMatchObject({ code: 'INVALID_RESPONSE' });
  });
});
