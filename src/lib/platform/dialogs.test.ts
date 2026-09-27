import { beforeEach, expect, test, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { confirm, open, save } from '@tauri-apps/plugin-dialog';
import { confirmAction, pickExportParentDirectory, pickExportSavePath, pickMarkdownFile, pickMarkdownSavePath, pickStartupDiagnosticsSavePath, pickWorkspaceDirectory } from './dialogs';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ confirm: vi.fn(), open: vi.fn(), save: vi.fn() }));
vi.mock('./runtime', () => ({ isTauriRuntime: () => true }));

beforeEach(() => { vi.resetAllMocks(); });

test('Android rejects desktop-only path dialogs before opening a native picker', async () => {
  vi.mocked(invoke).mockResolvedValue({ platform: 'android', desktopFiles: false, documentUris: false, exportFormats: [] });
  for (const pick of [pickMarkdownFile, pickMarkdownSavePath, pickStartupDiagnosticsSavePath, pickWorkspaceDirectory]) {
    await expect(pick()).rejects.toMatchObject({ code: 'CAPABILITY_UNAVAILABLE' });
  }
  expect(open).not.toHaveBeenCalled();
  expect(save).not.toHaveBeenCalled();
});

test('Android export paths use SAF document and tree resources', async () => {
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === 'get_platform_capabilities') return {
      platform: 'android', desktopFiles: false, documentUris: true, exportFormats: ['html', 'png']
    };
    if (command === 'pick_android_export_document') return {
      kind: 'androidDocument', uri: 'content://provider/document/output'
    };
    if (command === 'pick_android_tree') return {
      kind: 'androidTree', uri: 'content://provider/tree/output'
    };
    throw new Error(`Unexpected command: ${command}`);
  });
  await expect(pickExportSavePath('html', 'Article.html')).resolves.toBe('content://provider/document/output');
  await expect(pickExportParentDirectory()).resolves.toBe('content://provider/tree/output');
  expect(invoke).toHaveBeenCalledWith('pick_android_export_document', { format: 'html', title: 'Article.html' });
  expect(open).not.toHaveBeenCalled();
  expect(save).not.toHaveBeenCalled();
});

test('desktop selection retains native picker options and result', async () => {
  vi.mocked(invoke).mockResolvedValue({ platform: 'windows', desktopFiles: true, documentUris: false, exportFormats: ['pdf'] });
  vi.mocked(open).mockResolvedValue('C:\\notes\\sample.md');
  await expect(pickMarkdownFile()).resolves.toBe('C:\\notes\\sample.md');
  expect(open).toHaveBeenCalledWith({ multiple: false, filters: [{ name: 'Markdown', extensions: ['md', 'markdown', 'txt'] }] });
});

test('confirmation remains available without desktop file capability', async () => {
  vi.mocked(confirm).mockResolvedValue(true);
  await expect(confirmAction('Discard changes?')).resolves.toBe(true);
  expect(invoke).not.toHaveBeenCalled();
});

test('workspace picker requests a single directory and preserves cancellation', async () => {
  vi.mocked(invoke).mockResolvedValue({ platform: 'windows', desktopFiles: true, documentUris: false, exportFormats: [] });
  vi.mocked(open).mockResolvedValueOnce('C:\\notes').mockResolvedValueOnce(null);
  await expect(pickWorkspaceDirectory()).resolves.toBe('C:\\notes');
  expect(open).toHaveBeenCalledWith({ directory: true, multiple: false });
  await expect(pickWorkspaceDirectory()).resolves.toBeNull();
});
