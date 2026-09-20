import { describe, expect, it, vi } from 'vitest';
import { openDocumentPath } from './documentOpen';
import type { DocumentDto, DocumentOperationDto } from './tauriApi';

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise;
  });
  return { promise, resolve };
}

function operation(path: string): DocumentOperationDto {
  const document: DocumentDto = {
    path,
    fileIdentity: `test-file:${path}`,
    contentVersion: `sha256:${path}`,
    title: path.split('/').at(-1) ?? path,
    content: path,
    isDirty: false,
    lastSavedAt: null,
    fileSize: path.length
  };
  return { document, auxiliaryError: null };
}

describe('openDocumentPath', () => {
  it('returns the tab owned by each request even when another open changes the active tab', async () => {
    const opens = new Map([
      ['A.md', deferred<DocumentOperationDto>()],
      ['B.md', deferred<DocumentOperationDto>()]
    ]);
    const recentRefresh = deferred<void>();
    let activeTab = '';
    const openDocument = vi.fn((document: DocumentDto) => {
      activeTab = document.path ?? '';
      return { id: document.path };
    });
    const openFile = vi.fn((path: string) => opens.get(path)!.promise);
    const refresh = vi.fn(() => recentRefresh.promise);

    const openingA = openDocumentPath('A.md', openFile, openDocument, refresh);
    const openingB = openDocumentPath('B.md', openFile, openDocument, refresh);
    opens.get('A.md')!.resolve(operation('A.md'));
    const openedA = await openingA;
    opens.get('B.md')!.resolve(operation('B.md'));
    const openedB = await openingB;

    expect(openedA.tab).toEqual({ id: 'A.md' });
    expect(openedB.tab).toEqual({ id: 'B.md' });
    expect(activeTab).toBe('B.md');
    expect(refresh).toHaveBeenCalledTimes(2);
    recentRefresh.resolve();
  });
});
