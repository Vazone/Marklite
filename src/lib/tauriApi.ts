import { invoke } from '@tauri-apps/api/core';
import { open, save, confirm } from '@tauri-apps/plugin-dialog';
import { openUrl } from '@tauri-apps/plugin-opener';
import defaultSettingsJson from '../shared/default-settings.json';
import type { FrontendStartupEvent } from './startupLifecycle';

export type ThemeMode = 'light' | 'dark' | 'system';

export type AppSettings = {
  theme: ThemeMode;
  accentColor: string;
  editorFontFamily: string;
  previewFontFamily: string;
  editorFontSize: number;
  previewFontSize: number;
  lineHeight: number;
  interfaceScale: number;
  cornerRadius: number;
  showLineNumbers: boolean;
  wordWrap: boolean;
  tabSize: number;
  insertSpaces: boolean;
  autosaveEnabled: boolean;
  autosaveIntervalMs: number;
  livePreviewEnabled: boolean;
  previewDebounceMs: number;
  syncScroll: boolean;
  showSidebar: boolean;
  showStatusBar: boolean;
  restoreLastSession: boolean;
  recentFilesLimit: number;
  markdownToolbarEnabled: boolean;
  allowLocalImages: boolean;
  confirmExternalLinks: boolean;
};

export type DocumentDto = {
  path: string | null;
  title: string;
  content: string;
  isDirty: boolean;
  lastSavedAt: string | null;
  fileSize: number | null;
};

export type DocumentOperationDto = {
  document: DocumentDto;
  auxiliaryError: AppError | null;
};

export type RecentFileDto = {
  path: string;
  title: string;
  lastOpenedAt: string;
};

export type OutlineItem = {
  level: number;
  title: string;
  line: number;
  slug: string;
};

export type DocumentStats = {
  wordCount: number;
  characterCount: number;
  lineCount: number;
  headingCount: number;
  linkCount: number;
  imageCount: number;
};

export type RenderedMarkdownDto = {
  html: string;
  outline: OutlineItem[];
  stats: DocumentStats;
};

export type SessionStateDto = {
  version: 1;
  paths: string[];
  activePath: string | null;
};

export type MarkdownTargetDto =
  | { kind: 'anchor'; fragment: string }
  | { kind: 'localDocument'; path: string; fragment: string | null }
  | { kind: 'external'; url: string };

export type LocalImageDto = {
  dataUrl: string;
  path: string;
};

export type AppError = {
  code: string;
  message: string;
};

export type StartupReadyDto = {
  launchId: string;
  webviewVersion: string | null;
};

export type StartupDiagnosticsExportDto = {
  recordCount: number;
};

export const defaultSettings: AppSettings = {
  ...defaultSettingsJson,
  theme: defaultSettingsJson.theme as ThemeMode
};

export function isTauriRuntime(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

export function toAppError(error: unknown): AppError {
  if (typeof error === 'object' && error && 'message' in error) {
    const maybe = error as Partial<AppError>;
    return {
      code: maybe.code ?? 'UNKNOWN_ERROR',
      message: String(maybe.message)
    };
  }

  return {
    code: 'UNKNOWN_ERROR',
    message: String(error)
  };
}

export async function invokeCommand<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauriRuntime()) {
    return browserFallback<T>(command, args);
  }

  return invoke<T>(command, args);
}

export async function pickMarkdownFile(): Promise<string | null> {
  if (!isTauriRuntime()) {
    throw new Error('文件选择需要在 Tauri 桌面环境中使用');
  }

  const selected = await open({
    multiple: false,
    filters: [{ name: 'Markdown', extensions: ['md', 'markdown', 'txt'] }]
  });

  return Array.isArray(selected) ? selected[0] ?? null : selected;
}

export async function pickMarkdownSavePath(defaultPath?: string | null): Promise<string | null> {
  if (!isTauriRuntime()) {
    throw new Error('保存文件需要在 Tauri 桌面环境中使用');
  }

  return save({
    defaultPath: defaultPath ?? undefined,
    filters: [{ name: 'Markdown', extensions: ['md', 'markdown', 'txt'] }]
  });
}

export async function pickHtmlSavePath(defaultPath?: string | null): Promise<string | null> {
  if (!isTauriRuntime()) {
    throw new Error('导出 HTML 需要在 Tauri 桌面环境中使用');
  }

  return save({
    defaultPath: defaultPath ?? undefined,
    filters: [{ name: 'HTML', extensions: ['html'] }]
  });
}

export async function pickStartupDiagnosticsSavePath(): Promise<string | null> {
  if (!isTauriRuntime()) {
    throw new Error('导出启动诊断需要在 Tauri 桌面环境中使用');
  }

  return save({
    defaultPath: 'marklite-startup-diagnostics.json',
    filters: [{ name: 'JSON', extensions: ['json'] }]
  });
}

export async function confirmAction(message: string, title = 'MarkLite'): Promise<boolean> {
  if (!isTauriRuntime()) {
    return window.confirm(message);
  }

  return confirm(message, { title, kind: 'warning' });
}

export async function openExternalLink(url: string): Promise<void> {
  const parsed = new URL(url);
  if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') {
    throw { code: 'UNSUPPORTED_LINK_SCHEME', message: `不支持的链接协议：${parsed.protocol}` } satisfies AppError;
  }
  if (!isTauriRuntime()) {
    window.open(parsed.toString(), '_blank', 'noopener,noreferrer');
    return;
  }

  await openUrl(parsed.toString());
}

export const api = {
  openMarkdownFile: (path: string) => invokeCommand<DocumentOperationDto>('open_markdown_file', { path }),
  saveMarkdownFile: (path: string, content: string) =>
    invokeCommand<DocumentOperationDto>('save_markdown_file', { path, content }),
  exportHtmlFile: (path: string, title: string, content: string) =>
    invokeCommand<void>('export_html_file', { path, title, content }),
  getStartupFileArg: () => invokeCommand<string | null>('get_startup_file_arg'),
  renderMarkdown: (content: string) => invokeCommand<RenderedMarkdownDto>('render_markdown', { content }),
  resolveMarkdownTarget: (documentPath: string | null, target: string) =>
    invokeCommand<MarkdownTargetDto>('resolve_markdown_target', { documentPath, target }),
  loadLocalImage: (documentPath: string | null, target: string) =>
    invokeCommand<LocalImageDto>('load_local_image', { documentPath, target }),
  getSettings: () => invokeCommand<AppSettings>('get_settings'),
  updateSettings: (settings: AppSettings) => invokeCommand<AppSettings>('update_settings', { settings }),
  resetSettings: () => invokeCommand<AppSettings>('reset_settings'),
  getRecentFiles: () => invokeCommand<RecentFileDto[]>('get_recent_files'),
  removeRecentFile: (path: string) => invokeCommand<RecentFileDto[]>('remove_recent_file', { path }),
  clearMissingRecentFiles: () => invokeCommand<RecentFileDto[]>('clear_missing_recent_files'),
  getSession: () => invokeCommand<SessionStateDto>('get_session'),
  updateSession: (session: SessionStateDto) => invokeCommand<SessionStateDto>('update_session', { session }),
  clearSession: () => invokeCommand<void>('clear_session'),
  showInFileManager: (path: string) => invokeCommand<void>('show_in_file_manager', { path }),
  recordFrontendStartupEvent: (event: FrontendStartupEvent) =>
    invokeCommand<void>('record_frontend_startup_event', { event }),
  markFrontendReady: (elapsedMs: number) =>
    invokeCommand<StartupReadyDto>('mark_frontend_ready', { elapsedMs }),
  clearStartupDiagnostics: () => invokeCommand<void>('clear_startup_diagnostics'),
  exportStartupDiagnostics: (path: string) =>
    invokeCommand<StartupDiagnosticsExportDto>('export_startup_diagnostics', { path })
};

function browserFallback<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (command === 'get_settings') {
    return Promise.resolve(defaultSettings as T);
  }

  if (command === 'get_recent_files' || command === 'clear_missing_recent_files') {
    return Promise.resolve([] as T);
  }

  if (command === 'get_session') {
    return Promise.resolve({ version: 1, paths: [], activePath: null } as T);
  }

  if (command === 'update_session') {
    return Promise.resolve(args?.session as T);
  }

  if (command === 'clear_session') {
    return Promise.resolve(undefined as T);
  }

  if (command === 'record_frontend_startup_event' || command === 'clear_startup_diagnostics') {
    return Promise.resolve(undefined as T);
  }

  if (command === 'mark_frontend_ready') {
    return Promise.resolve({ launchId: 'browser', webviewVersion: null } as T);
  }

  if (command === 'render_markdown') {
    const content = String(args?.content ?? '');
    const html = content
      .split(/\n{2,}/)
      .map((block) => `<p>${escapeHtml(block).replace(/\n/g, '<br>')}</p>`)
      .join('');
    const headings = content
      .split('\n')
      .map((line, index) => ({ line, index }))
      .filter(({ line }) => /^#{1,6}\s+/.test(line))
      .map(({ line, index }) => ({
        level: line.match(/^#+/)?.[0].length ?? 1,
        title: line.replace(/^#+\s+/, ''),
        line: index + 1,
        slug: line.toLowerCase().replace(/[^a-z0-9]+/g, '-')
      }));

    return Promise.resolve({
      html,
      outline: headings,
      stats: {
        wordCount: content.split(/\s+/).filter(Boolean).length,
        characterCount: content.length,
        lineCount: content ? content.split('\n').length : 1,
        headingCount: headings.length,
        linkCount: (content.match(/\]\(/g) ?? []).length,
        imageCount: (content.match(/!\[/g) ?? []).length
      }
    } as T);
  }

  return Promise.reject(new Error('此操作需要在 Tauri 桌面环境中使用'));
}

function escapeHtml(value: string): string {
  return value
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#039;');
}
