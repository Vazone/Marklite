import { addPluginListener, invoke } from '@tauri-apps/api/core';
import { parseDocumentOperationDto, parseFileVersionDto } from './contractValidation';
import type { DocumentOperationDto, FileVersionDto, ExportFormat } from './contracts';
import { parseResourceRef, type ResourceRef } from './resources';

type Transport = (command: string, args?: Record<string, unknown>) => Promise<unknown>;

export type AndroidRecentDocument = {
  resource: { kind: 'androidDocument'; uri: string };
  title: string;
  lastOpenedAt: string;
};

function parseRecentDocuments(value: unknown): AndroidRecentDocument[] {
  if (!Array.isArray(value) || value.length > 50) throw { code: 'INVALID_RESPONSE', message: 'Invalid recent documents.' };
  return value.map(item => {
    if (!item || typeof item !== 'object' || Array.isArray(item)) {
      throw { code: 'INVALID_RESPONSE', message: 'Invalid recent document.' };
    }
    const entry = item as Record<string, unknown>;
    const resource = parseResourceRef(entry.resource);
    if (resource.kind !== 'androidDocument' || typeof entry.title !== 'string' || !entry.title.trim()
      || typeof entry.lastOpenedAt !== 'string' || !Number.isFinite(Date.parse(entry.lastOpenedAt))) {
      throw { code: 'INVALID_RESPONSE', message: 'Invalid recent document.' };
    }
    return { resource, title: entry.title, lastOpenedAt: entry.lastOpenedAt };
  });
}

export function createAndroidDocuments(call: Transport = invoke) {
  const pick = async (command: string, kind: 'androidDocument' | 'androidTree'): Promise<ResourceRef | null> => {
    const value = await call(command);
    if (value === null) return null;
    const resource = parseResourceRef(value);
    if (resource.kind !== kind) throw { code: 'INVALID_RESPONSE', message: 'Unexpected Android resource type.' };
    return resource;
  };
  return {
    async getRecentDocuments(): Promise<AndroidRecentDocument[]> {
      return parseRecentDocuments(await call('get_android_recent_documents'));
    },
    async removeRecentDocument(resource: ResourceRef): Promise<AndroidRecentDocument[]> {
      if (resource.kind !== 'androidDocument') throw { code: 'RESOURCE_UNSUPPORTED', message: 'Android document required.' };
      return parseRecentDocuments(await call('remove_android_recent_document', { resource }));
    },
    pickDocument: () => pick('pick_android_document', 'androidDocument'),
    async createExportDocument(format: Exclude<ExportFormat, 'png'>, title: string): Promise<ResourceRef | null> {
      const value = await call('pick_android_export_document', { format, title });
      if (value === null) return null;
      const resource = parseResourceRef(value);
      if (resource.kind !== 'androidDocument') throw { code: 'INVALID_RESPONSE', message: 'Android export document required.' };
      return resource;
    },
    createDocument: async (title: string) => {
      const value = await call('create_android_document', { title });
      if (value === null) return null;
      const resource = parseResourceRef(value);
      if (resource.kind !== 'androidDocument') throw { code: 'INVALID_RESPONSE', message: 'Unexpected Android resource type.' };
      return resource;
    },
    pickTree: () => pick('pick_android_tree', 'androidTree'),
    async open(resource: ResourceRef): Promise<DocumentOperationDto> {
      if (resource.kind !== 'androidDocument') throw { code: 'RESOURCE_UNSUPPORTED', message: 'Android document required.' };
      return parseDocumentOperationDto(await call('open_android_document', { resource }));
    },
    async version(resource: ResourceRef): Promise<FileVersionDto> {
      if (resource.kind !== 'androidDocument') throw { code: 'RESOURCE_UNSUPPORTED', message: 'Android document required.' };
      const version = parseFileVersionDto(await call('android_document_version', { resource }));
      if (!version) throw { code: 'INVALID_RESPONSE', message: 'Android version missing.' };
      return version;
    },
    async documentName(resource: ResourceRef): Promise<string | null> {
      if (resource.kind !== 'androidDocument') throw { code: 'RESOURCE_UNSUPPORTED', message: 'Android document required.' };
      const name = await call('android_document_name', { resource });
      if (name === null) return null;
      if (typeof name !== 'string' || name.length === 0 || new TextEncoder().encode(name).length > 255 ||
        /[\\/\x00-\x1f\x7f]/.test(name) || !/\.(md|markdown|txt)$/i.test(name)) {
        throw { code: 'INVALID_RESPONSE', message: 'Invalid Android document name.' };
      }
      return name;
    },
    async save(resource: ResourceRef, content: string, expectedFileIdentity: string | null,
      expectedContentVersion: string | null, overwriteContentConflict: boolean): Promise<DocumentOperationDto> {
      if (resource.kind !== 'androidDocument') throw { code: 'RESOURCE_UNSUPPORTED', message: 'Android document required.' };
      return parseDocumentOperationDto(await call('save_android_document', {
        resource, content, expectedFileIdentity, expectedContentVersion, overwriteContentConflict
      }));
    },
    async drainOpenRequests(): Promise<ResourceRef[]> {
      const response = await call('drain_android_open_requests');
      if (!Array.isArray(response) || response.length > 50) {
        throw { code: 'INVALID_RESPONSE', message: 'Invalid Android open request queue.' };
      }
      return response.map(value => {
        const resource = parseResourceRef(value);
        if (resource.kind !== 'androidDocument') throw { code: 'INVALID_RESPONSE', message: 'Invalid Android open request.' };
        return resource;
      });
    },
    async listenOpenRequests(onPending: () => void): Promise<() => void> {
      const listener = await addPluginListener<{ pending: boolean }>('marklite-mobile', 'openRequest', payload => {
        if (payload?.pending === true) onPending();
      });
      return () => { void listener.unregister(); };
    }
  };
}

export type AndroidDocuments = ReturnType<typeof createAndroidDocuments>;
