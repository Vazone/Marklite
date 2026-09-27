import { get } from 'svelte/store';
import { expect, test, vi } from 'vitest';
import { createDocumentStore } from '../stores/documentStore';
import { defaultExportOptions } from '../../lib/documentExport';
import { createExportController } from './exportController';

test('unmount during subscription releases late listener without exporting or updating UI', async () => {
  const store = createDocumentStore();
  store.newDocument();
  const tab = get(store).tabs.at(-1)!;
  let subscribed!: (unlisten: () => void) => void;
  const unlisten = vi.fn();
  const exportDocument = vi.fn();
  const setBusy = vi.fn();
  const closeDialog = vi.fn();
  const controller = createExportController({
    api: { getPlatformCapabilities: async () => ({ platform: 'windows', desktopFiles: true, documentUris: false, exportFormats: ['docx'] }), cancelExport: vi.fn(), analyzeMarkdown: vi.fn(), resolvePngExportDirectory: vi.fn(),
      exportDocument, suggestExportPath: vi.fn(), rememberExportDirectory: vi.fn() },
    flushEditor: vi.fn(), activeTab: () => tab, loadTab: vi.fn(),
    pickExportSavePath: vi.fn(), pickExportParentDirectory: vi.fn(),
    subscribe: () => new Promise(resolve => { subscribed = resolve; }),
    errorMessage: String, toast: vi.fn(), setBusy, setProgress: vi.fn(),
    setCancelRequested: vi.fn(), closeDialog, setWarnings: vi.fn()
  });
  const run = controller.run('docx', defaultExportOptions);
  await Promise.resolve();
  controller.dispose();
  subscribed(unlisten);
  await run;
  await controller.run('docx', defaultExportOptions);
  expect(unlisten).toHaveBeenCalledTimes(1);
  expect(exportDocument).not.toHaveBeenCalled();
  expect(closeDialog).not.toHaveBeenCalled();
  expect(setBusy.mock.calls).toEqual([[true]]);
});
