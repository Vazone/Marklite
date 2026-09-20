import type { ExportStage } from './exportProgress';
import type {
  ExportFormat,
  ExportOptions,
  ExportRequest,
  ExportResult,
  ExportSnapshot
} from './tauriApi';
import { t } from './i18n';
import type { ExportPathSuggestion } from './tauriApi';

export type ExportLocation = {
  suggest: (defaultPath: string) => Promise<ExportPathSuggestion>;
  remember: (targetPath: string) => Promise<void>;
  onWarning: (error: unknown) => void;
};

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
  mindMapSvg?: string;
};

export class ExportProtocolError extends Error {
  readonly code: string;

  constructor(code: string, message: string) {
    super(message);
    this.name = 'ExportProtocolError';
    this.code = code;
  }
}

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
    ? t('export.successWarnings', { format: name, count: warningCount })
    : t('export.success', { format: name });
}

export async function runExportJob(
  source: ExportSource,
  format: ExportFormat,
  options: ExportOptions,
  pickTarget: (defaultPath: string) => Promise<string | null>,
  execute: (request: ExportRequest) => Promise<ExportResult>,
  jobId = createExportJobId(),
  location?: ExportLocation,
  onPhase?: (stage: ExportStage) => void
): Promise<ExportResult | null> {
  onPhase?.('snapshot');
  const snapshot = freezeExportSnapshot(source, jobId);
  const frozenOptions = Object.freeze({ ...options });
  const mindMapSvg = format === 'svg' ? source.mindMapSvg ?? null : null;
  onPhase?.('preparingTarget');
  let suggestedPath = defaultExportPath(source, format);
  if (location && format !== 'png') {
    try {
      const suggestion = await location.suggest(suggestedPath);
      suggestedPath = suggestion.path;
      if (suggestion.warning) location.onWarning(suggestion.warning);
    } catch (error) {
      location.onWarning(error);
    }
  }
  onPhase?.(format === 'png' && snapshot.sourcePath ? 'preparingTarget' : 'choosingTarget');
  const targetPath = await pickTarget(suggestedPath);
  if (!targetPath) return null;
  onPhase?.(format === 'svg' ? 'validating' : 'parsing');
  const result = await execute({
    snapshot: { ...snapshot },
    targetPath,
    ...(format === 'png' ? { targetKind: 'directory' as const } : {}),
    format,
    options: { ...frozenOptions },
    mindMapSvg
  });
  onPhase?.('validating');
  if (
    result.jobId !== snapshot.jobId ||
    result.format !== format ||
    result.path !== targetPath ||
    (result.targetKind ?? 'file') !== (format === 'png' ? 'directory' : 'file')
  ) {
    throw new ExportProtocolError(
      'EXPORT_RESULT_MISMATCH',
      'The export response does not match the originating job and was ignored.'
    );
  }
  onPhase?.('finalizing');
  if (location && format !== 'png') {
    try { await location.remember(result.path); }
    catch (error) { location.onWarning(error); }
  }
  return result;
}
