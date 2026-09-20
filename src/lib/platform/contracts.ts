import defaultSettingsJson from '../../shared/default-settings.json';
import type { AppLanguage } from '../i18n';

export type ThemeMode = 'light' | 'dark' | 'system';

export type AppSettings = {
  language: AppLanguage;
  theme: ThemeMode;
  accentColor: string;
  editorFontFamily: string;
  previewFontFamily: string;
  editorFontSize: number;
  previewFontSize: number;
  lineHeight: number;
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
  fileIdentity: string | null;
  contentVersion: string | null;
  title: string;
  content: string;
  isDirty: boolean;
  lastSavedAt: string | null;
  fileSize: number | null;
};

export type FileVersionDto = {
  fileIdentity: string;
  contentVersion: string;
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

declare const sanitizedMarkdownHtmlBrand: unique symbol;
export type SanitizedMarkdownHtml = string & {
  readonly [sanitizedMarkdownHtmlBrand]: true;
};

export const EMPTY_SANITIZED_MARKDOWN_HTML = '' as SanitizedMarkdownHtml;

export type RenderedMarkdownDto = {
  html: SanitizedMarkdownHtml;
  outline: OutlineItem[];
  stats: DocumentStats;
  sourceBlocks: SourceBlock[];
  diagrams: DiagramSource[];
  diagramDiagnostics: DiagramDiagnostic[];
  virtualPreview?: VirtualPreviewIndex;
};

export type VirtualPreviewIndex = {
  sessionId: string;
  segments: VirtualPreviewSegment[];
};

export type VirtualPreviewSegment = {
  startUtf16: number;
  endUtf16: number;
  startLine: number;
  endLine: number;
  estimatedHeight: number;
  estimatedNodes: number;
};

export type VirtualPreviewWindow = {
  sessionId: string;
  start: number;
  end: number;
  segments: VirtualPreviewRenderedSegment[];
};

export type VirtualPreviewRenderedSegment = {
  index: number;
  html: SanitizedMarkdownHtml;
  sourceBlockStart: number;
  sourceBlocks: SourceBlock[];
  diagrams: DiagramSource[];
  diagramDiagnostics: DiagramDiagnostic[];
};

export type DiagramSource = {
  diagramId: string;
  ordinal: number;
  sourceUtf8: string;
  sourceSha256: string;
  sourceStartByte: number;
  sourceEndByte: number;
};

export type DiagramDiagnostic = {
  code: string;
  diagramId: string;
  sourceStartByte: number;
  sourceEndByte: number;
  message: string;
  retryable: boolean;
};

export type DiagramTheme = 'light' | 'dark';

export type RenderedDiagram = {
  diagramId: string;
  sourceSha256: string;
  cacheKey: string;
  rendererId: 'mermaid-offline-11.17.2';
  svgUtf8: string;
  width: number;
  height: number;
  viewBox: [number, number, number, number];
  accessibleTitle: string | null;
  accessibleDescription: string | null;
  warnings: DiagramDiagnostic[];
};

export type DiagramRuntimeAsset = {
  rendererId: 'mermaid-offline-11.17.2';
  scriptUtf8: string;
};

export type DiagramRuntimeStatus = {
  rendererId: 'mermaid-offline-11.17.2';
  installed: boolean;
};

export type SourceBlock = {
  startUtf16: number;
  endUtf16: number;
  startLine: number;
  endLine: number;
};

export type MarkdownAnalysisDto = {
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
  | { kind: 'external'; url: string }
  | { kind: 'email'; address: string };

export type LocalImageResourceDto = {
  objectUrl: string;
  path: string;
  width: number;
  height: number;
  encodedBytes: number;
  decodedBytes: number;
};

export type LocalImageBatchEntryDto = {
  target: string;
  resource: LocalImageResourceDto | null;
  error: AppError | null;
};

export type LocalImageBatchDto = {
  entries: LocalImageBatchEntryDto[];
  objectUrls: string[];
};

export type ExportFormat = 'html' | 'pdf' | 'docx' | 'svg' | 'png';
export type ExportPaperSize = 'a4' | 'letter';
export type ExportOrientation = 'portrait' | 'landscape';
export type ExportMarginPreset = 'narrow' | 'normal' | 'wide';

export type ExportSnapshot = {
  jobId: string;
  tabId: string;
  contentRevision: number;
  sourcePath: string | null;
  title: string;
  content: string;
};

export type ExportOptions = {
  paperSize: ExportPaperSize;
  orientation: ExportOrientation;
  margin: ExportMarginPreset;
  includeTitle: boolean;
  includeLocalImages: boolean;
};

export type ExportRequest = {
  snapshot: ExportSnapshot;
  targetPath: string;
  targetKind?: 'file' | 'directory';
  format: ExportFormat;
  options: ExportOptions;
  mindMapSvg: string | null;
};

export type ExportWarning = {
  code: string;
  message: string;
  target: string | null;
};

export type ExportResult = {
  targetKind?: 'file' | 'directory';
  jobId: string;
  format: ExportFormat;
  path: string;
  warnings: ExportWarning[];
};

export type ExportPathSuggestion = { path: string; warning: AppError | null };

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
  language: defaultSettingsJson.language as AppLanguage,
  theme: defaultSettingsJson.theme as ThemeMode
};
