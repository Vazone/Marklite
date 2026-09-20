import { confirm, open, save } from '@tauri-apps/plugin-dialog';
import type { ExportFormat } from './contracts';
import { isTauriRuntime } from './runtime';
import { t } from '../i18n';

export async function pickMarkdownFile(): Promise<string | null> {
  requireDesktopRuntime('File selection');
  const selected = await open({
    multiple: false,
    filters: [{ name: 'Markdown', extensions: ['md', 'markdown', 'txt'] }]
  });
  return Array.isArray(selected) ? selected[0] ?? null : selected;
}

export async function pickMarkdownSavePath(defaultPath?: string | null): Promise<string | null> {
  requireDesktopRuntime('Saving files');
  return save({
    defaultPath: defaultPath ?? undefined,
    filters: [{ name: 'Markdown', extensions: ['md', 'markdown', 'txt'] }]
  });
}

export async function pickExportSavePath(
  format: Exclude<ExportFormat, 'png'>,
  defaultPath?: string | null
): Promise<string | null> {
  requireDesktopRuntime('Exporting files');
  const labels: Record<Exclude<ExportFormat, 'png'>, string> = {
    html: 'HTML',
    pdf: 'PDF',
    docx: t('export.format.docx'),
    svg: t('export.format.svg')
  };
  return save({
    defaultPath: defaultPath ?? undefined,
    filters: [{ name: labels[format], extensions: [format] }]
  });
}

export async function pickStartupDiagnosticsSavePath(): Promise<string | null> {
  requireDesktopRuntime('Exporting startup diagnostics');
  return save({
    defaultPath: 'marklite-startup-diagnostics.json',
    filters: [{ name: 'JSON', extensions: ['json'] }]
  });
}

export async function confirmAction(message: string, title = 'MarkLite'): Promise<boolean> {
  if (!isTauriRuntime()) return window.confirm(message);
  return confirm(message, { title, kind: 'warning' });
}

function requireDesktopRuntime(action: string): void {
  if (!isTauriRuntime()) throw new Error(`${action} requires the Tauri desktop runtime.`);
}

export async function pickExportParentDirectory(): Promise<string | null> {
  requireDesktopRuntime('Exporting images');
  const selected = await open({ directory: true, multiple: false });
  return Array.isArray(selected) ? selected[0] ?? null : selected;
}
