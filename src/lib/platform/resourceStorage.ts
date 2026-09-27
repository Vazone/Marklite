import type { DocumentOperationDto, FileVersionDto } from './contracts';
import type { TauriClient } from './tauriClient';
import type { AndroidDocuments } from './androidDocuments';
import { desktopPath, type ResourceRef } from './resources';

export type ResourceWrite = {
  resource: ResourceRef;
  content: string;
  expectedFileIdentity: string | null;
  expectedContentVersion: string | null;
  overwriteContentConflict: boolean;
};

/** Resource access owns I/O; callers keep revisions, dirty state and conflict decisions. */
export type ResourceStorage = {
  read(resource: ResourceRef): Promise<DocumentOperationDto>;
  version(resource: ResourceRef, allowMissing: boolean): Promise<FileVersionDto | null>;
  write(request: ResourceWrite): Promise<DocumentOperationDto>;
};

export function createDesktopStorage(client: Pick<TauriClient,
  'openMarkdownFile' | 'resolveFileVersion' | 'saveMarkdownFile'>): ResourceStorage {
  return {
    async read(resource) { return client.openMarkdownFile(desktopPath(resource)); },
    async version(resource, allowMissing) { return client.resolveFileVersion(desktopPath(resource), allowMissing); },
    async write(request) {
      return client.saveMarkdownFile(desktopPath(request.resource), request.content,
        request.expectedFileIdentity, request.expectedContentVersion, request.overwriteContentConflict);
    }
  };
}

export function createResourceStorage(client: Pick<TauriClient,
  'openMarkdownFile' | 'resolveFileVersion' | 'saveMarkdownFile'>,
  android: Pick<AndroidDocuments, 'open' | 'version' | 'save'>): ResourceStorage {
  const desktop = createDesktopStorage(client);
  return {
    read: resource => resource.kind === 'androidDocument' ? android.open(resource) : desktop.read(resource),
    version: (resource, allowMissing) => resource.kind === 'androidDocument'
      ? android.version(resource) : desktop.version(resource, allowMissing),
    write: request => request.resource.kind === 'androidDocument'
      ? android.save(request.resource, request.content, request.expectedFileIdentity,
          request.expectedContentVersion, request.overwriteContentConflict)
      : desktop.write(request)
  };
}
