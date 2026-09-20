import type { ExportResult } from './tauriApi';
import type { ExportSource } from './documentExport';

export type LocatedExportWarning = ExportResult['warnings'][number] & {
  line: number | null;
  excerpt: string | null;
};

export type ExportWarningReport = {
  jobId: string;
  format: ExportResult['format'];
  outputPath: string;
  documentTitle: string;
  sourcePath: string | null;
  tabId: string;
  contentRevision: number;
  warnings: LocatedExportWarning[];
};

function sourceLocation(content: string, target: string | null): Pick<LocatedExportWarning, 'line' | 'excerpt'> {
  const span = target?.match(/:(\d+)-(\d+)$/);
  if (!span) return { line: null, excerpt: null };
  const start = Number(span[1]);
  const end = Number(span[2]);
  if (!Number.isSafeInteger(start) || !Number.isSafeInteger(end) || start > end) {
    return { line: null, excerpt: null };
  }
  let bytePosition = 0;
  let line = 1;
  for (const text of content.split('\n')) {
    const lineBytes = new TextEncoder().encode(text).length;
    if (start >= bytePosition && start < bytePosition + lineBytes + 1) {
      return { line, excerpt: text.trim().slice(0, 160) || null };
    }
    bytePosition += lineBytes + 1;
    line++;
  }
  return { line: null, excerpt: null };
}

export function exportWarningReport(source: ExportSource, result: ExportResult): ExportWarningReport {
  return {
    jobId: result.jobId,
    format: result.format,
    outputPath: result.path,
    documentTitle: source.title,
    sourcePath: source.path,
    tabId: source.id,
    contentRevision: source.contentRevision,
    warnings: result.warnings.map((warning) => ({
      ...warning,
      ...sourceLocation(source.content, warning.target)
    }))
  };
}

export function serializeExportWarningReport(report: ExportWarningReport): string {
  return [
    `${report.documentTitle} — ${report.format.toUpperCase()} — ${report.jobId}`,
    `Source: ${report.sourcePath ?? '(unsaved)'} (revision ${report.contentRevision})`,
    `Output: ${report.outputPath}`,
    ...report.warnings.map((warning, index) =>
      `${index + 1}. [${warning.code}] ${warning.message}\n` +
      `   ${warning.target ?? '(document)'}${warning.line === null ? '' : ` (line ${warning.line})`}` +
      (warning.excerpt ? `\n   ${warning.excerpt}` : '')
    )
  ].join('\n');
}
