import { listen } from '@tauri-apps/api/event';

export function listenWorkspaceChanges(receive: (value: unknown) => void): Promise<() => void> {
  return listen('workspace-invalidated', event => receive(event.payload));
}
