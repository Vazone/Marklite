import type { ExportFormat } from './platform/contracts';

export const exportStages = ['loading', 'snapshot', 'preparingTarget', 'choosingTarget', 'parsing', 'resources', 'rendering', 'printing', 'encoding', 'merging', 'validating', 'writing', 'committing', 'cleaningUp', 'finalizing'] as const;
export type ExportStage = typeof exportStages[number];
export type ExportStatus = 'running' | 'waiting' | 'succeeded' | 'failed' | 'cancelled';
export type ExportWork = { kind: 'chapter' | 'part'; index: number; total: number; completed: number };
export type ExportProgressView = { jobId: string; format: ExportFormat; stage: ExportStage; status: ExportStatus; work: ExportWork | null };
type ExportProgressEvent = ExportProgressView & { sequence: number };
type Subscribe = (receive: (value: unknown) => void) => Promise<() => void>;

function record(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

export function parseExportProgress(value: unknown): ExportProgressEvent | null {
  if (!record(value) || typeof value.jobId !== 'string' || value.jobId.length > 512
      || !['html', 'pdf', 'docx', 'svg', 'png'].includes(String(value.format))
      || !exportStages.includes(value.stage as ExportStage)
      || !['running', 'waiting', 'succeeded', 'failed', 'cancelled'].includes(String(value.status))
      || !Number.isSafeInteger(value.sequence) || (value.sequence as number) <= 0) return null;
  let work: ExportWork | null = null;
  if (value.work !== null) {
    if (!record(value.work)) return null;
    const candidate = value.work;
    if (!['chapter', 'part'].includes(String(candidate.kind))
        || ![candidate.index, candidate.total, candidate.completed].every(Number.isSafeInteger)
        || (candidate.total as number) < 1 || (candidate.index as number) < 1
        || (candidate.index as number) > (candidate.total as number)
        || (candidate.completed as number) < 0 || (candidate.completed as number) > (candidate.index as number)) return null;
    work = { kind: candidate.kind as ExportWork['kind'], index: candidate.index as number, total: candidate.total as number, completed: candidate.completed as number };
  }
  return { jobId: value.jobId, format: value.format as ExportFormat, sequence: value.sequence as number, stage: value.stage as ExportStage, status: value.status as ExportStatus, work };
}

/** One task owns one subscription and one current view. The result promise,
 * not a backend terminal event, is the final authority for GUI completion. */
export class ExportProgressTask {
  private sequence = 0;
  private closed = false;
  private backendClosed = false;
  private unlisten: (() => void) | null = null;
  private view: ExportProgressView;

  constructor(jobId: string, format: ExportFormat, private readonly change: (view: ExportProgressView) => void) {
    this.view = { jobId, format, stage: 'loading', status: 'running', work: null };
    this.publish();
  }

  async subscribe(subscribe: Subscribe): Promise<void> {
    const unlisten = await subscribe((value) => this.receive(value));
    if (this.closed) unlisten();
    else this.unlisten = unlisten;
  }

  phase(stage: ExportStage): void {
    if (this.closed) return;
    this.view = { ...this.view, stage, status: stage === 'choosingTarget' ? 'waiting' : 'running', work: null };
    this.publish();
  }

  private receive(value: unknown): void {
    const event = parseExportProgress(value);
    if (this.closed || this.backendClosed || !event || event.jobId !== this.view.jobId
        || event.format !== this.view.format || event.sequence <= this.sequence) return;
    this.sequence = event.sequence;
    const terminal = !['running', 'waiting'].includes(event.status);
    this.backendClosed = terminal;
    this.view = { jobId: event.jobId, format: event.format, stage: terminal ? 'finalizing' : event.stage, status: terminal ? 'running' : event.status, work: terminal ? null : event.work };
    this.publish();
  }

  finish(status: 'succeeded' | 'failed' | 'cancelled'): void {
    if (this.closed) return;
    this.view = { ...this.view, status, stage: 'finalizing', work: null };
    this.publish();
    this.dispose();
  }

  dispose(): void {
    this.closed = true;
    this.unlisten?.();
    this.unlisten = null;
  }

  private publish(): void { this.change({ ...this.view }); }
}
