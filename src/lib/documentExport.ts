import type {
  ExportFormat,
  ExportOptions,
  ExportRequest,
  ExportResult,
  ExportSnapshot
} from './tauriApi';

export const defaultExportOptions: ExportOptions = {
  paperSize: 'a4',
  orientation: 'portrait',
  margin: 'normal',
  includeTitle: true,
  includeLocalImages: false
};

export type ExportSource = {
  id: string;
  path: string | null;
  title: string;
  content: string;
  contentRevision: number;
};

export function createExportJobId(now = Date.now(), random = Math.random()): string {
  return `export-${now.toString(36)}-${random.toString(16).slice(2)}`;
}

export function freezeExportSnapshot(
  source: ExportSource,
  jobId = createExportJobId()
): Readonly<ExportSnapshot> {
  return Object.freeze({
    jobId,
    tabId: source.id,
    contentRevision: source.contentRevision,
    sourcePath: source.path,
    title: source.title,
    content: source.content
  });
}

export function defaultExportPath(source: ExportSource, format: ExportFormat): string {
  const fallbackBase = source.title.replace(/\.(md|markdown|txt)$/i, '') || 'Untitled';
  if (!source.path) return `${fallbackBase}.${format}`;
  const replaced = source.path.replace(/\.(md|markdown|txt)$/i, `.${format}`);
  return replaced === source.path ? `${source.path}.${format}` : replaced;
}

export function exportSuccessMessage(format: ExportFormat, warningCount: number): string {
  const name = format.toUpperCase();
  return warningCount > 0
    ? `${name} 已导出（${warningCount} 项内容已降级）`
    : `${name} 已导出`;
}

export async function runExportJob(
  source: ExportSource,
  format: ExportFormat,
  options: ExportOptions,
  pickTarget: (defaultPath: string) => Promise<string | null>,
  execute: (request: ExportRequest) => Promise<ExportResult>,
  jobId = createExportJobId()
): Promise<ExportResult | null> {
  const snapshot = freezeExportSnapshot(source, jobId);
  const targetPath = await pickTarget(defaultExportPath(source, format));
  if (!targetPath) return null;
  const result = await execute({
    snapshot: { ...snapshot },
    targetPath,
    format,
    options: { ...options }
  });
  if (result.jobId !== snapshot.jobId) {
    throw {
      code: 'EXPORT_JOB_MISMATCH',
      message: '导出响应与发起任务不匹配，已忽略该响应'
    };
  }
  return result;
}
