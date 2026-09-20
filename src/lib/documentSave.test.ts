import { describe, expect, it, vi } from 'vitest';
import type { DocumentOperationDto } from './tauriApi';
import {
  saveDocumentSnapshot,
  type DocumentSaveDependencies,
  type SaveableTabSnapshot
} from './documentSave';

function operation(fileIdentity = 'file:saved'): DocumentOperationDto {
  return {
    document: {
      path: 'C:\\docs\\saved.md',
      fileIdentity,
      contentVersion: 'sha256:saved',
      title: 'saved.md',
      content: '# saved',
      isDirty: false,
      lastSavedAt: null,
      fileSize: 7
    },
    auxiliaryError: null
  };
}

function dependencies(
  overrides: Partial<DocumentSaveDependencies> = {}
): DocumentSaveDependencies {
  return {
    getTab: vi.fn((): SaveableTabSnapshot => ({
      id: 'tab-a',
      path: 'C:\\docs\\saved.md',
      fileIdentity: 'file:target',
      contentVersion: 'sha256:opened',
      loadState: 'loaded',
      content: '# snapshot',
      contentRevision: 4
    })),
    resolveFileVersion: vi.fn(async () => ({
      fileIdentity: 'file:target',
      contentVersion: 'sha256:opened'
    })),
    getFileOwner: vi.fn(() => undefined),
    activateTab: vi.fn(),
    saveFile: vi.fn(async () => operation()),
    markSaved: vi.fn(() => true),
    ...overrides
  };
}

describe('saveDocumentSnapshot', () => {
  it('passes the observed target identity into the atomic backend save', async () => {
    const deps = dependencies();

    await expect(saveDocumentSnapshot('tab-a', 'C:\\docs\\saved.md', deps)).resolves.toEqual(
      operation()
    );

    expect(deps.saveFile).toHaveBeenCalledWith(
      'C:\\docs\\saved.md',
      '# snapshot',
      'file:target',
      'sha256:opened',
      false
    );
    expect(deps.markSaved).toHaveBeenCalledWith('tab-a', 4, operation().document);
  });

  it('activates an existing owner and rejects before writing', async () => {
    const activateTab = vi.fn();
    const saveFile = vi.fn(async () => operation());
    const deps = dependencies({
      getFileOwner: vi.fn(() => ({ id: 'tab-owner', title: 'owner.md' })),
      activateTab,
      saveFile
    });

    await expect(saveDocumentSnapshot('tab-a', 'C:\\docs\\saved.md', deps)).rejects.toMatchObject({
      code: 'FILE_ALREADY_OPEN'
    });
    expect(activateTab).toHaveBeenCalledWith('tab-owner');
    expect(saveFile).not.toHaveBeenCalled();
  });

  it('rejects a store ownership conflict instead of reporting a false success', async () => {
    const deps = dependencies({ markSaved: vi.fn(() => false) });

    await expect(saveDocumentSnapshot('tab-a', 'C:\\docs\\saved.md', deps)).rejects.toMatchObject({
      code: 'FILE_IDENTITY_CONFLICT'
    });
  });

  it('propagates a content conflict without committing the saved response', async () => {
    const markSaved = vi.fn(() => true);
    const saveFile = vi.fn(async () => {
      throw Object.assign(new Error('external change'), { code: 'FILE_CONTENT_CHANGED' });
    });
    const deps = dependencies({ saveFile, markSaved });

    await expect(saveDocumentSnapshot('tab-a', 'C:\\docs\\saved.md', deps)).rejects.toMatchObject({
      code: 'FILE_CONTENT_CHANGED'
    });
    expect(markSaved).not.toHaveBeenCalled();
  });

  it('rejects an original path replaced before preflight', async () => {
    const saveFile = vi.fn(async () => operation());
    const deps = dependencies({
      resolveFileVersion: vi.fn(async () => ({
        fileIdentity: 'file:replacement',
        contentVersion: 'sha256:replacement'
      })),
      saveFile
    });

    await expect(saveDocumentSnapshot('tab-a', 'C:\\docs\\saved.md', deps)).rejects.toMatchObject({
      code: 'FILE_TARGET_CHANGED'
    });
    expect(saveFile).not.toHaveBeenCalled();
  });

  it('uses the observed identity when Save As intentionally targets another file', async () => {
    const deps = dependencies({
      resolveFileVersion: vi.fn(async () => ({
        fileIdentity: 'file:other-target',
        contentVersion: 'sha256:other'
      }))
    });

    await saveDocumentSnapshot('tab-a', 'C:\\docs\\other.md', deps);

    expect(deps.saveFile).toHaveBeenCalledWith(
      'C:\\docs\\other.md',
      '# snapshot',
      'file:other-target',
      'sha256:other',
      false
    );
  });

  it('makes explicit overwrite traceable while still binding to the latest observed target', async () => {
    const deps = dependencies({
      resolveFileVersion: vi.fn(async () => ({
        fileIdentity: 'file:replacement',
        contentVersion: 'sha256:replacement'
      }))
    });

    await saveDocumentSnapshot('tab-a', 'C:\\docs\\saved.md', deps, true);

    expect(deps.saveFile).toHaveBeenCalledWith(
      'C:\\docs\\saved.md',
      '# snapshot',
      'file:replacement',
      'sha256:replacement',
      true
    );
  });
});
