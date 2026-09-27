import type { AppError, ExportFormat } from './contracts';

export type PlatformCapabilities = {
  platform: string;
  desktopFiles: boolean;
  documentUris: boolean;
  exportFormats: ExportFormat[];
};

export const browserCapabilities: PlatformCapabilities = {
  platform: 'browser', desktopFiles: false, documentUris: false, exportFormats: []
};

export function parseCapabilities(value: unknown): PlatformCapabilities {
  const item = value as Partial<PlatformCapabilities> | null;
  const formats = ['html', 'pdf', 'docx', 'svg', 'png'];
  if (!item || typeof item.platform !== 'string' || !item.platform
    || typeof item.desktopFiles !== 'boolean' || typeof item.documentUris !== 'boolean'
    || !Array.isArray(item.exportFormats) || item.exportFormats.some(format => !formats.includes(format))
    || new Set(item.exportFormats).size !== item.exportFormats.length) {
    throw { code: 'INVALID_RESPONSE', message: 'Invalid platform capabilities.' } satisfies AppError;
  }
  return { platform: item.platform, desktopFiles: item.desktopFiles, documentUris: item.documentUris, exportFormats: [...item.exportFormats] };
}

export function requireExport(capabilities: PlatformCapabilities, format: ExportFormat): void {
  if (!capabilities.exportFormats.includes(format)) {
    throw { code: 'CAPABILITY_UNAVAILABLE', message: `Exporting ${format.toUpperCase()} is unavailable on this platform.` } satisfies AppError;
  }
}
