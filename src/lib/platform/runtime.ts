import { invoke } from '@tauri-apps/api/core';
import { invokeBrowserCommand } from './browserAdapter';

export type CommandTransport = {
  invoke: (command: string, args?: Record<string, unknown>) => Promise<unknown>;
};

export function isTauriRuntime(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

export const runtimeCommandTransport: CommandTransport = {
  invoke(command, args) {
    return isTauriRuntime() ? invoke<unknown>(command, args) : invokeBrowserCommand(command, args);
  }
};
