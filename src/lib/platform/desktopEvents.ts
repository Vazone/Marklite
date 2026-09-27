import { getCurrentWindow } from '@tauri-apps/api/window';

export function listenExportProgress(receive: (value: unknown) => void): Promise<() => void> {
  return getCurrentWindow().listen('document-export-progress', (event) => receive(event.payload));
}

export const desktopEvents = {
  onDrop(receive: (paths: string[]) => void): Promise<() => void> {
    return getCurrentWindow().onDragDropEvent((event) => {
      if (event.payload.type === 'drop') receive(event.payload.paths);
    });
  },
  onOpen(receive: () => void): Promise<() => void> {
    return getCurrentWindow().listen('single-instance-open-file', receive);
  },
  onClose(receive: (preventDefault: () => void) => void): Promise<() => void> {
    return getCurrentWindow().onCloseRequested(event => receive(() => event.preventDefault()));
  },
  destroy: () => getCurrentWindow().destroy()
};
