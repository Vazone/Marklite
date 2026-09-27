import type { FrontendStartupEvent } from '../startupLifecycle';
import { createWorkspaceClient, type WorkspaceClient } from './workspaceClient';
import { desktopFile, parseResourceRef, type ResourceRef } from './resources';
import { parseRecoveryInventory, parseRecoveryReceipt, parseRecoverySnapshot, type RecoveryInventory, type RecoveryReceipt, type RecoverySnapshot } from './recovery';
import type { ExportCancelStatus, ExportFormat } from './contracts';
import { parseCapabilities, type PlatformCapabilities } from './capabilities';
import {
  decodeLocalImageResponse,
  parseAppSettings,
  parseDocumentOperationDto,
  parseExportResult,
  parseExportPathSuggestion,
  parseFileVersionDto,
  parseMarkdownTarget,
  parseMarkdownAnalysisDto,
  parseOptionalString,
  parseRecentFiles,
  parseRenderedMarkdownDto,
  parseVirtualPreviewWindow,
  parseRenderedDiagram,
  parseDiagramRuntimeAsset,
  parseDiagramRuntimeStatus,
  parseSessionState,
  parseResourceSession,
  parseStartupDiagnosticsExport,
  parseStartupReady,
  parseString,
  parseBoolean,
  parseStringArray
} from './contractValidation';
import type {
  AppSettings,
  DocumentOperationDto,
  ExportRequest,
  ExportResult,
  ExportPathSuggestion,
  FileVersionDto,
  LocalImageBatchDto,
  MarkdownTargetDto,
  MarkdownAnalysisDto,
  RecentFileDto,
  RenderedMarkdownDto,
  VirtualPreviewWindow,
  RenderedDiagram,
  DiagramRuntimeAsset,
  DiagramRuntimeStatus,
  SessionStateDto,
  ResourceSessionDto,
  StartupDiagnosticsExportDto,
  StartupReadyDto
} from './contracts';
import type { CommandTransport } from './runtime';

export type TauriClient = {
  workspace: WorkspaceClient;
  listRecovery(): Promise<RecoveryInventory>;
  readRecovery(id: string): Promise<RecoverySnapshot>;
  saveRecovery(snapshot: RecoverySnapshot): Promise<RecoveryReceipt>;
  resolveRecovery(receipt: RecoveryReceipt): Promise<void>;
  cancelExport(jobId: string, format: ExportFormat): Promise<ExportCancelStatus>;
  getPlatformCapabilities(): Promise<PlatformCapabilities>;
  openMarkdownFile(path: string): Promise<DocumentOperationDto>;
  saveMarkdownFile(
    path: string,
    content: string,
    expectedFileIdentity: string | null,
    expectedContentVersion: string | null,
    overwriteContentConflict: boolean
  ): Promise<DocumentOperationDto>;
  resolveFileIdentity(path: string, allowMissing: boolean): Promise<string | null>;
  resolveFileVersion(path: string, allowMissing: boolean): Promise<FileVersionDto | null>;
  resolvePngExportDirectory(sourcePath: string | null, title: string, parentPath: string | null): Promise<string>;
  cancelPngExport(jobId: string): Promise<boolean>;
  exportDocument(request: ExportRequest, sourceResource?: ResourceRef | null): Promise<ExportResult>;
  suggestExportPath(defaultPath: string): Promise<ExportPathSuggestion>;
  rememberExportDirectory(targetPath: string): Promise<void>;
  getStartupFileArg(): Promise<string | null>;
  renderMarkdown(content: string, tabId?: string, contentRevision?: number): Promise<RenderedMarkdownDto>;
  renderMarkdownWindow(sessionId: string, start: number, end: number): Promise<VirtualPreviewWindow>;
  releaseMarkdownPreview(sessionId: string): Promise<void>;
  validateDiagramSvg(diagram: RenderedDiagram): Promise<RenderedDiagram>;
  getDiagramRuntimeStatus(): Promise<DiagramRuntimeStatus>;
  loadDiagramRuntime(): Promise<DiagramRuntimeAsset>;
  installDiagramRuntime(packPath: string): Promise<DiagramRuntimeStatus>;
  uninstallDiagramRuntime(): Promise<DiagramRuntimeStatus>;
  analyzeMarkdown(content: string): Promise<MarkdownAnalysisDto>;
  resolveMarkdownTarget(documentPath: string | null, target: string): Promise<MarkdownTargetDto>;
  openValidatedEmailLink(address: string): Promise<void>;
  loadLocalImages(documentPath: string | null, targets: string[], jobId: string): Promise<LocalImageBatchDto>;
  cancelLocalImageJob(jobId: string): Promise<void>;
  drainOpenFileRequests(): Promise<string[]>;
  getSettings(): Promise<AppSettings>;
  updateSettings(settings: AppSettings): Promise<AppSettings>;
  resetSettings(): Promise<AppSettings>;
  getRecentFiles(): Promise<RecentFileDto[]>;
  removeRecentFile(path: string): Promise<RecentFileDto[]>;
  clearMissingRecentFiles(): Promise<RecentFileDto[]>;
  getSession(): Promise<SessionStateDto>;
  updateSession(session: SessionStateDto): Promise<SessionStateDto>;
  clearSession(): Promise<void>;
  getResourceSession(): Promise<ResourceSessionDto>;
  updateResourceSession(session: ResourceSessionDto): Promise<ResourceSessionDto>;
  clearResourceSession(): Promise<void>;
  showInFileManager(path: string): Promise<void>;
  recordFrontendStartupEvent(event: FrontendStartupEvent): Promise<void>;
  markFrontendReady(elapsedMs: number): Promise<StartupReadyDto>;
  clearStartupDiagnostics(): Promise<void>;
  exportStartupDiagnostics(path: string): Promise<StartupDiagnosticsExportDto>;
};

export function createTauriClient(
  transport: CommandTransport,
  createObjectUrl?: (blob: Blob) => string
): TauriClient {
  const invoke = (command: string, args?: Record<string, unknown>) => transport.invoke(command, args);
  const invokeUnit = async (command: string, args?: Record<string, unknown>) => {
    await invoke(command, args);
  };
  const getPlatformCapabilities = async () => parseCapabilities(await invoke('get_platform_capabilities'));

  return {
    workspace: createWorkspaceClient(transport, getPlatformCapabilities),
    async listRecovery() { return parseRecoveryInventory(await invoke('list_recovery')); },
    async readRecovery(id) { return parseRecoverySnapshot(await invoke('read_recovery', { id })); },
    async saveRecovery(snapshot) { return parseRecoveryReceipt(await invoke('save_recovery', { snapshot })); },
    async resolveRecovery(receipt) { return invokeUnit('resolve_recovery', { receipt }); },
    async cancelExport(jobId, format) {
      const status = await invoke('cancel_export', { jobId, format });
      if (status === 'requested' || status === 'notRunning' || status === 'tooLate' || status === 'unsupported') return status;
      throw { code: 'INVALID_RESPONSE', message: 'Invalid cancellation acknowledgement.' };
    },
    getPlatformCapabilities,
    async openMarkdownFile(path) {
      return parseDocumentOperationDto(await invoke('open_markdown_file', { path }));
    },
    async saveMarkdownFile(
      path,
      content,
      expectedFileIdentity,
      expectedContentVersion,
      overwriteContentConflict
    ) {
      return parseDocumentOperationDto(
        await invoke('save_markdown_file', {
          path,
          content,
          expectedFileIdentity,
          expectedContentVersion,
          overwriteContentConflict
        })
      );
    },
    async resolveFileIdentity(path, allowMissing) {
      return parseOptionalString(
        await invoke('resolve_file_identity', { path, allowMissing }),
        'resolve_file_identity'
      );
    },
    async resolveFileVersion(path, allowMissing) {
      return parseFileVersionDto(await invoke('resolve_file_version', { path, allowMissing }));
    },
    async resolvePngExportDirectory(sourcePath, title, parentPath) {
      return parseString(await invoke('resolve_png_export_directory', { sourcePath, title, parentPath }), 'resolve_png_export_directory');
    },
    async cancelPngExport(jobId) {
      return parseBoolean(await invoke('cancel_png_export', { jobId }), 'cancel_png_export');
    },
    async exportDocument(request, sourceResource) {
      return parseExportResult(await invoke('export_document', {
        request,
        sourceResource: sourceResource === undefined
          ? request.snapshot.sourcePath === null ? null : desktopFile(request.snapshot.sourcePath)
          : sourceResource === null ? null : parseResourceRef(sourceResource),
        targetResource: request.targetPath.startsWith('content://')
          ? parseResourceRef({ kind: request.format === 'png' ? 'androidTree' : 'androidDocument', uri: request.targetPath })
          : desktopFile(request.targetPath)
      }));
    },
    async suggestExportPath(defaultPath) {
      return parseExportPathSuggestion(await invoke('suggest_export_path', { defaultPath }));
    },
    rememberExportDirectory(targetPath) {
      return invokeUnit('remember_export_directory', { targetPath });
    },
    async getStartupFileArg() {
      return parseOptionalString(await invoke('get_startup_file_arg'), 'get_startup_file_arg');
    },
    async renderMarkdown(content, tabId, contentRevision) {
      return parseRenderedMarkdownDto(await invoke('render_markdown', {
        content,
        ...(tabId === undefined ? {} : { tabId }),
        ...(contentRevision === undefined ? {} : { contentRevision })
      }));
    },
    async renderMarkdownWindow(sessionId, start, end) {
      return parseVirtualPreviewWindow(await invoke('render_markdown_window', { sessionId, start, end }));
    },
    async releaseMarkdownPreview(sessionId) {
      return invokeUnit('release_markdown_preview', { sessionId });
    },
    async validateDiagramSvg(diagram) {
      return parseRenderedDiagram(await invoke('validate_diagram_svg', { diagram }));
    },
    async getDiagramRuntimeStatus() {
      return parseDiagramRuntimeStatus(await invoke('get_diagram_runtime_status'));
    },
    async loadDiagramRuntime() {
      return parseDiagramRuntimeAsset(await invoke('load_diagram_runtime'));
    },
    async installDiagramRuntime(packPath) {
      return parseDiagramRuntimeStatus(await invoke('install_diagram_runtime', { packPath }));
    },
    async uninstallDiagramRuntime() {
      return parseDiagramRuntimeStatus(await invoke('uninstall_diagram_runtime'));
    },
    async analyzeMarkdown(content) {
      return parseMarkdownAnalysisDto(await invoke('analyze_markdown', { content }));
    },
    async resolveMarkdownTarget(documentPath, target) {
      return parseMarkdownTarget(await invoke('resolve_markdown_target', { documentPath, target }));
    },
    openValidatedEmailLink(address) {
      return invokeUnit('open_validated_email_link', { address });
    },
    async loadLocalImages(documentPath, targets, jobId) {
      return decodeLocalImageResponse(
        await invoke('load_local_image', { documentPath, targets, jobId }),
        createObjectUrl
      );
    },
    cancelLocalImageJob(jobId) {
      return invokeUnit('cancel_local_image_job', { jobId });
    },
    async drainOpenFileRequests() {
      return parseStringArray(await invoke('drain_open_file_requests'), 'drain_open_file_requests');
    },
    async getSettings() {
      return parseAppSettings(await invoke('get_settings'));
    },
    async updateSettings(settings) {
      return parseAppSettings(await invoke('update_settings', { settings }));
    },
    async resetSettings() {
      return parseAppSettings(await invoke('reset_settings'));
    },
    async getRecentFiles() {
      return parseRecentFiles(await invoke('get_recent_files'));
    },
    async removeRecentFile(path) {
      return parseRecentFiles(await invoke('remove_recent_file', { path }));
    },
    async clearMissingRecentFiles() {
      return parseRecentFiles(await invoke('clear_missing_recent_files'));
    },
    async getSession() {
      return parseSessionState(await invoke('get_session'));
    },
    async updateSession(session) {
      return parseSessionState(await invoke('update_session', { session }));
    },
    clearSession() {
      return invokeUnit('clear_session');
    },
    async getResourceSession() {
      return parseResourceSession(await invoke('get_resource_session'));
    },
    async updateResourceSession(session) {
      return parseResourceSession(await invoke('update_resource_session', { session }));
    },
    clearResourceSession() {
      return invokeUnit('clear_resource_session');
    },
    showInFileManager(path) {
      return invokeUnit('show_in_file_manager', { path });
    },
    recordFrontendStartupEvent(event) {
      return invokeUnit('record_frontend_startup_event', { event });
    },
    async markFrontendReady(elapsedMs) {
      return parseStartupReady(await invoke('mark_frontend_ready', { elapsedMs }));
    },
    clearStartupDiagnostics() {
      return invokeUnit('clear_startup_diagnostics');
    },
    async exportStartupDiagnostics(path) {
      return parseStartupDiagnosticsExport(await invoke('export_startup_diagnostics', { path }));
    }
  };
}
