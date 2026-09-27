import { ExportProgressTask, type ExportProgressView } from '../../lib/exportProgress';
import { runExportJob, createExportJobId } from '../../lib/documentExport';
import { exportWarningReport, type ExportWarningReport } from '../../lib/exportWarningReport';
import { serializeMindMapSvg } from '../../lib/mindMapSvg';
import { t } from '../../lib/i18n';
import { toAppError, type api as Api, type ExportFormat, type ExportOptions } from '../../lib/tauriApi';
import type { EditorTab } from '../stores/documentStore';
import { requireExport } from '../../lib/platform/capabilities';

type Dependencies = {
  api: Pick<typeof Api, 'getPlatformCapabilities' | 'cancelExport' | 'analyzeMarkdown' | 'resolvePngExportDirectory' | 'exportDocument' | 'suggestExportPath' | 'rememberExportDirectory'>;
  flushEditor: () => void;
  activeTab: () => EditorTab | null;
  loadTab: (id: string) => Promise<EditorTab | null>;
  pickExportSavePath: (format: Exclude<ExportFormat, 'png'>, path: string) => Promise<string | null>;
  pickExportParentDirectory: () => Promise<string | null>;
  subscribe?: (receive: (value: unknown) => void) => Promise<() => void>;
  errorMessage: (error: unknown) => string;
  toast: (message: string, kind?: 'error' | 'info' | 'success') => void;
  setBusy: (busy: boolean) => void;
  setProgress: (view: ExportProgressView | null) => void;
  setCancelRequested: (requested: boolean) => void;
  closeDialog: () => void;
  setWarnings: (report: ExportWarningReport | null) => void;
};

/** Owns one export invocation, its progress subscription and native cancellation. */
export function createExportController(deps: Dependencies) {
  const { api, flushEditor, activeTab, loadTab, pickExportSavePath, pickExportParentDirectory,
    subscribe, errorMessage, toast, setBusy, setProgress, setCancelRequested, closeDialog, setWarnings } = deps;
  let busy = false;
  let disposed = false;
  let exportTask: ExportProgressTask | null = null;
  let cancellableJob: { id: string; format: ExportFormat; executing: boolean; cancelRequested: boolean } | null = null;
  async function cancel() {
    const job = cancellableJob;
    if (!job || job.cancelRequested) return;
    cancellableJob = { ...job, cancelRequested: true };
    if (!disposed) setCancelRequested(true);
    const pending = cancellableJob;
    try {
      // Registration happens in the backend. Retry only while this invocation is live.
      while (cancellableJob === pending && pending.executing) {
        const status = await api.cancelExport(pending.id, pending.format);
        // Only the export result/progress can confirm completion. Retry registration races only.
        if (status !== 'notRunning') return;
        await new Promise((resolve) => setTimeout(resolve, 50));
      }
    } catch (error) {
      if (!disposed) toast(errorMessage(error), 'error');
    }
  }

  async function run(format: ExportFormat, options: ExportOptions) {
    if (busy || disposed) return;
    flushEditor();
    let tab = activeTab();
    if (!tab) return;
    busy = true;
    setBusy(true);
    const jobId = createExportJobId();
    const task = new ExportProgressTask(jobId, format, (view) => { if (!disposed) setProgress(view); });
    exportTask = task;
    if (format === 'png' || format === 'pdf') cancellableJob = { id: jobId, format, executing: false, cancelRequested: false };
    try {
      const capabilities = await api.getPlatformCapabilities();
      requireExport(capabilities, format);
      if (disposed) return;
      if (subscribe) {
        try { await task.subscribe(subscribe); }
        catch (error) { console.warn('Export progress subscription unavailable', error); }
      }
      if (disposed) return;
      if (tab.loadState !== 'loaded') {
        const loadedTab = await loadTab(tab.id);
        if (disposed || !loadedTab || loadedTab.loadState !== 'loaded') return;
        tab = loadedTab;
      }
      task.phase(format === 'svg' ? 'rendering' : 'snapshot');
      const exportSource = format === 'svg'
        ? {
            ...tab,
            mindMapSvg: serializeMindMapSvg(
              tab.title,
              (await api.analyzeMarkdown(tab.content)).outline
            )
          }
        : { ...tab };
      const locationWarnings: string[] = [];
      const result = await runExportJob(
        exportSource,
        format,
        options,
        async (defaultPath) => {
          if (disposed) return null;
          if (format !== 'png') return pickExportSavePath(format, defaultPath);
          const parent = capabilities.platform === 'android' || !exportSource.path
            ? await pickExportParentDirectory() : null;
          if ((capabilities.platform === 'android' && !parent) ||
            (!exportSource.path && !parent) || cancellableJob?.cancelRequested) return null;
          return api.resolvePngExportDirectory(exportSource.path, exportSource.title, parent);
        },
        async (request) => {
          if (disposed) throw { code: 'EXPORT_CANCELLED', message: 'Export owner disposed' };
          if (cancellableJob) {
            if (cancellableJob.cancelRequested) throw { code: 'EXPORT_CANCELLED', message: 'Export cancelled' };
            cancellableJob.executing = true;
          }
          return api.exportDocument(request, tab?.resource ?? null);
        },
        jobId,
        capabilities.desktopFiles ? {
          suggest: api.suggestExportPath,
          remember: api.rememberExportDirectory,
          onWarning: (error) => locationWarnings.push(errorMessage(error))
        } : undefined,
        (stage) => task.phase(stage)
      );
      if (disposed) return;
      if (!result) {
        task.finish('cancelled');
        closeDialog();
        if (locationWarnings.length) toast(locationWarnings.join('\n'), 'error');
        return;
      }
      task.finish('succeeded');
      closeDialog();
      setWarnings(result.warnings.length ? exportWarningReport(exportSource, result) : null);
      const successMessage = result.warnings.length
        ? t('export.successWarnings', { format: format.toUpperCase(), count: result.warnings.length })
        : t('export.success', { format: format.toUpperCase() });
      toast([successMessage, ...(format === 'png' ? [result.path] : []), ...locationWarnings].join('\n'));
    } catch (error) {
      const cancelled = toAppError(error).code === 'EXPORT_CANCELLED';
      task.finish(cancelled ? 'cancelled' : 'failed');
      if (!disposed) toast(errorMessage(error), cancelled ? 'info' : 'error');
    } finally {
      task.dispose();
      exportTask = null;
      if (!disposed) setProgress(null);
      busy = false;
      if (!disposed) setBusy(false);
      cancellableJob = null;
      if (!disposed) setCancelRequested(false);
    }
  }

  return {
    run, cancel,
    dispose() {
      disposed = true;
      if (cancellableJob) void cancel();
      exportTask?.dispose();
    }
  };
}
