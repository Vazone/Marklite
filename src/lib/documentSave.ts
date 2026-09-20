import type { DocumentDto, DocumentOperationDto, FileVersionDto } from './tauriApi';
import { isSameFileIdentity, isSameFilePath } from './filePathIdentity';

export type SaveableTabSnapshot = {
  id: string;
  path: string | null;
  fileIdentity: string | null;
  contentVersion: string | null;
  loadState: 'loaded' | 'unloaded' | 'loading' | 'error';
  content: string;
  contentRevision: number;
};

export type FileOwner = {
  id: string;
  title: string;
};

export type DocumentSaveDependencies = {
  getTab: (tabId: string) => SaveableTabSnapshot | undefined;
  resolveFileVersion: (path: string, allowMissing: boolean) => Promise<FileVersionDto | null>;
  getFileOwner: (fileIdentity: string, excludingTabId: string) => FileOwner | undefined;
  activateTab: (tabId: string) => void;
  saveFile: (
    path: string,
    content: string,
    expectedFileIdentity: string | null,
    expectedContentVersion: string | null,
    overwriteContentConflict: boolean
  ) => Promise<DocumentOperationDto>;
  markSaved: (tabId: string, contentRevision: number, document: DocumentDto) => boolean;
};

function saveError(code: string, message: string): Error & { code: string } {
  return Object.assign(new Error(message), { code });
}

/**
 * Executes one ownership-checked document save. Request coalescing, retrying a
 * newer revision, recent-file refresh, and user notifications remain with the
 * application coordinator; this function owns the filesystem identity
 * handshake and store-commit invariant.
 */
export async function saveDocumentSnapshot(
  tabId: string,
  path: string,
  dependencies: DocumentSaveDependencies,
  overwriteExternalChanges = false
): Promise<DocumentOperationDto | null> {
  const tab = dependencies.getTab(tabId);
  if (!tab || tab.loadState !== 'loaded') return null;

  const observedVersion = await dependencies.resolveFileVersion(path, true);
  const observedFileIdentity = observedVersion?.fileIdentity ?? null;
  const targetsOriginalPath = isSameFilePath(tab.path, path);
  if (
    targetsOriginalPath &&
    tab.fileIdentity &&
    !overwriteExternalChanges &&
    !isSameFileIdentity(tab.fileIdentity, observedFileIdentity)
  ) {
    throw saveError(
      'FILE_TARGET_CHANGED',
      'The document file changed after it was opened; overwrite was blocked.'
    );
  }
  if (observedFileIdentity) {
    const owner = dependencies.getFileOwner(observedFileIdentity, tabId);
    if (owner) {
      dependencies.activateTab(owner.id);
      throw saveError(
        'FILE_ALREADY_OPEN',
        `The file is already open as “${owner.title}”; that tab was activated and no file was overwritten.`
      );
    }
  }

  const expectedFileIdentity = targetsOriginalPath && !overwriteExternalChanges
    ? tab.fileIdentity
    : observedFileIdentity;
  const expectedContentVersion = targetsOriginalPath && !overwriteExternalChanges
    ? tab.contentVersion
    : observedVersion?.contentVersion ?? null;
  const result = await dependencies.saveFile(
    path,
    tab.content,
    expectedFileIdentity,
    expectedContentVersion,
    overwriteExternalChanges
  );
  if (!dependencies.markSaved(tabId, tab.contentRevision, result.document)) {
    throw saveError(
      'FILE_IDENTITY_CONFLICT',
      'The saved file identity conflicts with another open document; tab state was not changed.'
    );
  }
  return result;
}
