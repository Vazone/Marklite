import type { CommandTransport } from './runtime';
import { parseResourceRef, type ResourceRef } from './resources';
import type { PlatformCapabilities } from './capabilities';
import { parseDirectoryRevision, parseWorkspacePage, parseWorkspaceRoot, parseWorkspacePreferences, type WorkspacePreferences, type PageCursor } from './workspace';

/** Filesystem-free port. UI code passes user intent, and never assembles native commands. */
export function createWorkspaceClient(transport: CommandTransport, getCapabilities: () => Promise<PlatformCapabilities>) {
  const invoke = (command: string, args: Record<string, unknown>) => transport.invoke(command, args);
  const isAndroid = async () => (await getCapabilities()).platform === 'android';
  return {
    async loadPreferences() { return parseWorkspacePreferences(await (await isAndroid()
      ? invoke('get_android_workspace_preferences', {}) : invoke('get_workspace_preferences', {}))); },
    async savePreferences(preferences: WorkspacePreferences) {
      return parseWorkspacePreferences(await (await isAndroid()
        ? invoke('save_android_workspace_preferences', { preferences })
        : invoke('save_workspace_preferences', { preferences })));
    },
    async mount(resource: ResourceRef) {
      const capabilities = await getCapabilities();
      if (resource.kind === 'androidTree' && capabilities.platform === 'android' && capabilities.documentUris) {
        return parseWorkspaceRoot(await invoke('mount_android_workspace', { resource }));
      }
      if (!capabilities.desktopFiles || resource.kind !== 'desktopDirectory') {
        throw { code: 'CAPABILITY_UNAVAILABLE', message: 'Directory workspaces are unavailable on this platform.' };
      }
      return parseWorkspaceRoot(await invoke('mount_workspace', { resource }));
    },
    async unmount(rootId: string): Promise<void> { if (await isAndroid()) await invoke('unmount_android_workspace', { rootId });
      else await invoke('unmount_workspace', { rootId }); },
    async list(rootId: string, relativePath: string, cursor: PageCursor | null, requestId: string, limit = 256) {
      const args = { rootId, relativePath, cursor, requestId, limit };
      return parseWorkspacePage(await (await isAndroid()
        ? invoke('list_android_workspace', args) : invoke('list_workspace', args)));
    },
    async cancel(rootId: string, requestId: string): Promise<void> { if (await isAndroid()) await invoke('cancel_android_workspace', { rootId, requestId });
      else await invoke('cancel_workspace', { rootId, requestId }); },
    async refresh(rootId: string, relativePath: string) {
      return parseDirectoryRevision(await (await isAndroid()
        ? invoke('refresh_android_workspace', { rootId, relativePath })
        : invoke('refresh_workspace', { rootId, relativePath })));
    },
    async collapse(rootId: string, relativePath: string): Promise<void> { if (await isAndroid()) await invoke('collapse_android_workspace', { rootId, relativePath });
      else await invoke('collapse_workspace', { rootId, relativePath }); },
    async resolve(rootId: string, relativePath: string) {
      const resource = parseResourceRef(await (await isAndroid()
        ? invoke('resolve_android_workspace_file', { rootId, relativePath })
        : invoke('resolve_workspace_file', { rootId, relativePath })));
      if (resource.kind !== 'desktopFile' && resource.kind !== 'androidDocument') {
        throw { code: 'INVALID_RESPONSE', message: 'Workspace entry is not a document.' };
      }
      return resource;
    }
  };
}

export type WorkspaceClient = ReturnType<typeof createWorkspaceClient>;
