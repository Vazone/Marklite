import { invoke } from '@tauri-apps/api/core';
import { parseCapabilities } from './capabilities';
import { confirm, open, save } from '@tauri-apps/plugin-dialog';
import type { ExportFormat } from './contracts';
import { isTauriRuntime } from './runtime';
import { t } from '../i18n';
import { createAndroidDocuments } from './androidDocuments';

export async function pickMarkdownFile(): Promise<string | null> {
  await requireDesktopRuntime('File selection');
  const selected = await open({
    multiple: false,
    filters: [{ name: 'Markdown', extensions: ['md', 'markdown', 'txt'] }]
  });
  return Array.isArray(selected) ? selected[0] ?? null : selected;
}

export async function pickMarkdownSavePath(defaultPath?: string | null): Promise<string | null> {
  await requireDesktopRuntime('Saving files');
  return save({
    defaultPath: defaultPath ?? undefined,
    filters: [{ name: 'Markdown', extensions: ['md', 'markdown', 'txt'] }]
  });
}

export async function pickExportSavePath(
  format: Exclude<ExportFormat, 'png'>,
  defaultPath?: string | null
): Promise<string | null> {
  if (isTauriRuntime()) {
    const capabilities = parseCapabilities(await invoke('get_platform_capabilities'));
    if (capabilities.platform === 'android') {
      const name = (defaultPath ?? `Untitled.${format}`).split(/[\\/]/).at(-1) ?? `Untitled.${format}`;
      const resource = await createAndroidDocuments().createExportDocument(format, name);
      return resource?.kind === 'androidDocument' ? resource.uri : null;
    }
  }
  await requireDesktopRuntime('Exporting files');
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
  await requireDesktopRuntime('Exporting startup diagnostics');
  return save({
    defaultPath: 'marklite-startup-diagnostics.json',
    filters: [{ name: 'JSON', extensions: ['json'] }]
  });
}

export async function confirmAction(message: string, title = 'MarkLite'): Promise<boolean> {
  if (!isTauriRuntime()) return window.confirm(message);
  return confirm(message, { title, kind: 'warning' });
}

async function requireDesktopRuntime(action: string): Promise<void> {
  if (!isTauriRuntime()) throw new Error(`${action} requires the Tauri desktop runtime.`);
  const capabilities = parseCapabilities(await invoke('get_platform_capabilities'));
  if (!capabilities.desktopFiles) {
    throw { code: 'CAPABILITY_UNAVAILABLE', message: `${action} is not yet available on ${capabilities.platform}.` };
  }
}

export async function pickExportParentDirectory(): Promise<string | null> {
  if (isTauriRuntime()) {
    const capabilities = parseCapabilities(await invoke('get_platform_capabilities'));
    if (capabilities.platform === 'android') {
      const resource = await createAndroidDocuments().pickTree();
      return resource?.kind === 'androidTree' ? resource.uri : null;
    }
  }
  return pickDirectory('Exporting images');
}

export async function pickDiagramPackPath(): Promise<string | null> {
  await requireDesktopRuntime('Installing the Mermaid pack');
  const selected = await open({
    multiple: false,
    filters: [{ name: 'Mermaid pack', extensions: ['zip'] }]
  });
  return Array.isArray(selected) ? selected[0] ?? null : selected;
}

export async function pickWorkspaceDirectory(): Promise<string | null> {
  return pickDirectory('Opening folders');
}

async function pickDirectory(action: string): Promise<string | null> {
  await requireDesktopRuntime(action);
  const selected = await open({ directory: true, multiple: false });
  return Array.isArray(selected) ? selected[0] ?? null : selected;
}
