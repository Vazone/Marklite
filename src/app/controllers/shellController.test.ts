import { get } from 'svelte/store';
import { expect, test, vi } from 'vitest';
import { defaultSettings } from '../../lib/tauriApi';
import { createDocumentStore } from '../stores/documentStore';
import { createShellController } from './shellController';

test('closing flushes the current session before window destruction and keeps failure recoverable', async () => {
  const store = createDocumentStore();
  const calls: string[] = [];
  let fail = true;
  const destroy = vi.fn(async () => { calls.push('destroy'); });
  const onKey = vi.fn();
  const controller = createShellController({
    api: { recordFrontendStartupEvent: vi.fn(), clearSession: vi.fn(), getSession: vi.fn(), getStartupFileArg: vi.fn(),
      getPlatformCapabilities: vi.fn(), getResourceSession: vi.fn(), clearResourceSession: vi.fn() },
    loadSettings: async () => true, settings: () => defaultSettings, state: () => get(store),
    setSidebarVisible: vi.fn(), refreshRecentFiles: async () => true, restoreTabs: vi.fn(),
    restoreResourceTabs: vi.fn(), onResourceSessionMode: vi.fn(),
    sessionCoordinator: { queue: () => { calls.push('queue'); }, setWritable: vi.fn(), flush: async () => {
      calls.push('persist'); if (fail) throw Error('disk unavailable');
    } },
    onSessionReady: vi.fn(), setupExternalOpenListener: async () => true,
    openPath: vi.fn(), renderActiveNow: vi.fn(), setupDragDrop: async () => true,
    flushEditor: () => { calls.push('editor'); }, destroyWindow: destroy, onError: vi.fn(), onKey
  });
  controller.mount();
  await expect(controller.close()).rejects.toThrow('disk unavailable');
  expect(destroy).not.toHaveBeenCalled();
  fail = false; calls.length = 0;
  await controller.close();
  expect(calls).toEqual(['editor', 'queue', 'persist', 'destroy']);
  window.dispatchEvent(new KeyboardEvent('keydown'));
  expect(onKey).toHaveBeenCalledTimes(1);
  controller.dispose();
  window.dispatchEvent(new KeyboardEvent('keydown'));
  expect(onKey).toHaveBeenCalledTimes(1);
});
