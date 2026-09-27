import { describe, expect, it, vi } from 'vitest';
import { prepareInstall } from './installBarrier';

function fixture() {
  let dirty = ['draft'];
  let busy = false;
  const saveDocument = vi.fn(async () => { dirty = []; return true; });
  const flushPersistence = vi.fn(async () => {});
  const confirmSave = vi.fn(async () => true);
  const deps = {
    flushEditor: vi.fn(), isBusy: () => busy, dirtyDocuments: () => dirty,
    confirmSave, saveDocument, flushPersistence
  };
  return { deps, saveDocument, confirmSave, flushPersistence,
    setBusy: (value: boolean) => { busy = value; },
    setDirty: (value: string[]) => { dirty = value; } };
}

describe('update installation barrier', () => {
  it('saves dirty documents and flushes persistent state before installation', async () => {
    const { deps, saveDocument, flushPersistence } = fixture();
    expect(await prepareInstall(deps)).toBe('ready');
    expect(saveDocument).toHaveBeenCalledWith('draft');
    expect(flushPersistence).toHaveBeenCalledOnce();
  });

  it('leaves the document and running tasks alone after cancellation or busy state', async () => {
    const { deps, confirmSave, saveDocument, flushPersistence, setBusy } = fixture();
    confirmSave.mockResolvedValueOnce(false);
    expect(await prepareInstall(deps)).toBe('cancelled');
    setBusy(true);
    expect(await prepareInstall(deps)).toBe('busy');
    expect(saveDocument).not.toHaveBeenCalled();
    expect(flushPersistence).not.toHaveBeenCalled();
  });

  it('blocks installation if saving fails or new edits appear during flush', async () => {
    const { deps, saveDocument, flushPersistence, setDirty } = fixture();
    saveDocument.mockResolvedValueOnce(false);
    expect(await prepareInstall(deps)).toBe('save-failed');
    flushPersistence.mockImplementationOnce(async () => { setDirty(['new edit']); });
    expect(await prepareInstall(deps)).toBe('save-failed');
  });
});
