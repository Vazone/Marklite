import type {
  AppError,
  AppSettings,
  DocumentDto,
  DocumentOperationDto,
  DocumentStats,
  ExportFormat,
  ExportResult,
  ExportPathSuggestion,
  ExportWarning,
  FileVersionDto,
  LocalImageBatchDto,
  MarkdownTargetDto,
  OutlineItem,
  RecentFileDto,
  RenderedMarkdownDto,
  DiagramSource,
  DiagramDiagnostic,
  DiagramRuntimeAsset,
  DiagramRuntimeStatus,
  RenderedDiagram,
  MarkdownAnalysisDto,
  SanitizedMarkdownHtml,
  SourceBlock,
  SessionStateDto,
  StartupDiagnosticsExportDto,
  StartupReadyDto,
  ThemeMode,
  VirtualPreviewIndex,
  VirtualPreviewWindow
} from './contracts';

export function toAppError(error: unknown): AppError {
  if (isRecord(error) && 'message' in error) {
    return {
      code: typeof error.code === 'string' ? error.code : 'UNKNOWN_ERROR',
      message: String(error.message)
    };
  }

  return {
    code: 'UNKNOWN_ERROR',
    message: String(error)
  };
}

export function parseExportPathSuggestion(value: unknown): ExportPathSuggestion {
  if (!isRecord(value)) throw contractError('export_path_suggestion');
  return {
    path: parseString(value.path, 'export_path_suggestion.path'),
    warning: value.warning === null ? null : parseAppError(value.warning, 'export_path_suggestion.warning')
  };
}

export function parseDocumentOperationDto(value: unknown): DocumentOperationDto {
  if (!isRecord(value)) throw contractError('document_operation');
  return {
    document: parseDocumentDto(value.document),
    auxiliaryError: value.auxiliaryError === null ? null : parseAppError(value.auxiliaryError, 'document_operation.auxiliaryError')
  };
}

export function parseFileVersionDto(value: unknown): FileVersionDto | null {
  if (value === null) return null;
  if (!isRecord(value)) throw contractError('file_version');
  return {
    fileIdentity: parseString(value.fileIdentity, 'file_version.fileIdentity'),
    contentVersion: parseString(value.contentVersion, 'file_version.contentVersion')
  };
}

export function parseRenderedMarkdownDto(value: unknown): RenderedMarkdownDto {
  if (!isRecord(value) || typeof value.html !== 'string') {
    throw contractError('render_markdown.html');
  }
  if (!Array.isArray(value.outline)) throw contractError('render_markdown.outline');
  if (!Array.isArray(value.sourceBlocks)) throw contractError('render_markdown.sourceBlocks');
  if (!Array.isArray(value.diagrams)) throw contractError('render_markdown.diagrams');
  if (!Array.isArray(value.diagramDiagnostics)) throw contractError('render_markdown.diagramDiagnostics');

  const sourceBlocks = value.sourceBlocks.map((block, index) =>
    parseSourceBlock(block, `render_markdown.sourceBlocks[${index}]`)
  );
  if (sourceBlocks.some((block, index) => index > 0 && block.startUtf16 < sourceBlocks[index - 1].endUtf16)) {
    throw contractError('render_markdown.sourceBlocks.order');
  }

  return {
    html: value.html as SanitizedMarkdownHtml,
    outline: value.outline.map((item, index) => parseOutlineItem(item, `render_markdown.outline[${index}]`)),
    stats: parseDocumentStats(value.stats, 'render_markdown.stats'),
    sourceBlocks,
    diagrams: value.diagrams.map((diagram, index) =>
      parseDiagramSource(diagram, `render_markdown.diagrams[${index}]`, index)
    ),
    diagramDiagnostics: value.diagramDiagnostics.map((diagnostic, index) =>
      parseDiagramDiagnostic(diagnostic, `render_markdown.diagramDiagnostics[${index}]`)
    ),
    virtualPreview: value.virtualPreview === undefined
      ? undefined
      : parseVirtualPreviewIndex(value.virtualPreview)
  };
}

function parseVirtualPreviewIndex(value: unknown): VirtualPreviewIndex {
  if (!isRecord(value) || !Array.isArray(value.segments)) throw contractError('render_markdown.virtualPreview');
  const sessionId = parseString(value.sessionId, 'render_markdown.virtualPreview.sessionId');
  if (!sessionId) throw contractError('render_markdown.virtualPreview.sessionId');
  let previousEnd = 0;
  const segments = value.segments.map((segment, index) => {
    const field = `render_markdown.virtualPreview.segments[${index}]`;
    if (!isRecord(segment)) throw contractError(field);
    const startUtf16 = parseNonNegativeInteger(segment.startUtf16, `${field}.startUtf16`);
    const endUtf16 = parseNonNegativeInteger(segment.endUtf16, `${field}.endUtf16`);
    const startLine = parseNonNegativeInteger(segment.startLine, `${field}.startLine`);
    const endLine = parseNonNegativeInteger(segment.endLine, `${field}.endLine`);
    const estimatedHeight = parseNonNegativeInteger(segment.estimatedHeight, `${field}.estimatedHeight`);
    const estimatedNodes = parseNonNegativeInteger(segment.estimatedNodes, `${field}.estimatedNodes`);
    if (endUtf16 < startUtf16 || endLine < startLine || startLine < 1 || estimatedHeight < 1 ||
      (index > 0 && startUtf16 < previousEnd)) throw contractError(field);
    previousEnd = endUtf16;
    return { startUtf16, endUtf16, startLine, endLine, estimatedHeight, estimatedNodes };
  });
  return { sessionId, segments };
}

export function parseVirtualPreviewWindow(value: unknown): VirtualPreviewWindow {
  if (!isRecord(value) || !Array.isArray(value.segments)) throw contractError('render_markdown_window');
  const sessionId = parseString(value.sessionId, 'render_markdown_window.sessionId');
  const start = parseNonNegativeInteger(value.start, 'render_markdown_window.start');
  const end = parseNonNegativeInteger(value.end, 'render_markdown_window.end');
  if (!sessionId || end <= start || end - start > 12 || value.segments.length !== end - start) {
    throw contractError('render_markdown_window.range');
  }
  const segments = value.segments.map((segment, offset) => {
    const field = `render_markdown_window.segments[${offset}]`;
    if (!isRecord(segment) || typeof segment.html !== 'string' || !Array.isArray(segment.sourceBlocks) ||
      !Array.isArray(segment.diagrams) || !Array.isArray(segment.diagramDiagnostics)) throw contractError(field);
    const index = parseNonNegativeInteger(segment.index, `${field}.index`);
    if (index !== start + offset) throw contractError(`${field}.index`);
    return {
      index,
      html: segment.html as SanitizedMarkdownHtml,
      sourceBlockStart: parseNonNegativeInteger(segment.sourceBlockStart, `${field}.sourceBlockStart`),
      sourceBlocks: segment.sourceBlocks.map((block, blockIndex) => parseSourceBlock(block, `${field}.sourceBlocks[${blockIndex}]`)),
      diagrams: segment.diagrams.map((diagram, diagramIndex) => parseDiagramSource(diagram, `${field}.diagrams[${diagramIndex}]`, null)),
      diagramDiagnostics: segment.diagramDiagnostics.map((diagnostic, diagnosticIndex) => parseDiagramDiagnostic(diagnostic, `${field}.diagramDiagnostics[${diagnosticIndex}]`))
    };
  });
  return { sessionId, start, end, segments };
}

function parseDiagramDiagnostic(value: unknown, field: string): DiagramDiagnostic {
  if (!isRecord(value)) throw contractError(field);
  const sourceStartByte = parseNonNegativeInteger(value.sourceStartByte, `${field}.sourceStartByte`);
  const sourceEndByte = parseNonNegativeInteger(value.sourceEndByte, `${field}.sourceEndByte`);
  if (sourceEndByte < sourceStartByte) throw contractError(field);
  return {
    code: parseString(value.code, `${field}.code`),
    diagramId: parseString(value.diagramId, `${field}.diagramId`),
    sourceStartByte,
    sourceEndByte,
    message: parseString(value.message, `${field}.message`),
    retryable: parseBoolean(value.retryable, `${field}.retryable`)
  };
}

export function parseRenderedDiagram(value: unknown): RenderedDiagram {
  if (!isRecord(value) || !Array.isArray(value.viewBox) || value.viewBox.length !== 4) {
    throw contractError('rendered_diagram');
  }
  const rendererId = parseString(value.rendererId, 'rendered_diagram.rendererId');
  if (rendererId !== 'mermaid-offline-11.17.2') throw contractError('rendered_diagram.rendererId');
  return {
    diagramId: parseString(value.diagramId, 'rendered_diagram.diagramId'),
    sourceSha256: parseString(value.sourceSha256, 'rendered_diagram.sourceSha256'),
    cacheKey: parseString(value.cacheKey, 'rendered_diagram.cacheKey'),
    rendererId,
    svgUtf8: parseString(value.svgUtf8, 'rendered_diagram.svgUtf8'),
    width: parseFiniteNumber(value.width, 'rendered_diagram.width'),
    height: parseFiniteNumber(value.height, 'rendered_diagram.height'),
    viewBox: value.viewBox.map((item, index) =>
      parseFiniteNumber(item, `rendered_diagram.viewBox[${index}]`)
    ) as [number, number, number, number],
    accessibleTitle: parseOptionalString(value.accessibleTitle, 'rendered_diagram.accessibleTitle'),
    accessibleDescription: parseOptionalString(
      value.accessibleDescription,
      'rendered_diagram.accessibleDescription'
    ),
    warnings: Array.isArray(value.warnings)
      ? value.warnings.map((warning, index) =>
          parseDiagramDiagnostic(warning, `rendered_diagram.warnings[${index}]`)
        )
      : (() => {
          throw contractError('rendered_diagram.warnings');
        })()
  };
}

export function parseDiagramRuntimeAsset(value: unknown): DiagramRuntimeAsset {
  if (!isRecord(value) || value.rendererId !== 'mermaid-offline-11.17.2') {
    throw contractError('diagram_runtime_asset');
  }
  return {
    rendererId: value.rendererId,
    scriptUtf8: parseString(value.scriptUtf8, 'diagram_runtime_asset.scriptUtf8')
  };
}

export function parseDiagramRuntimeStatus(value: unknown): DiagramRuntimeStatus {
  if (!isRecord(value) || value.rendererId !== 'mermaid-offline-11.17.2') {
    throw contractError('diagram_runtime_status');
  }
  return {
    rendererId: value.rendererId,
    installed: parseBoolean(value.installed, 'diagram_runtime_status.installed')
  };
}

function parseDiagramSource(value: unknown, field: string, expectedOrdinal: number | null): DiagramSource {
  if (!isRecord(value)) throw contractError(field);
  const diagramId = parseString(value.diagramId, `${field}.diagramId`);
  const ordinal = parseNonNegativeInteger(value.ordinal, `${field}.ordinal`);
  const sourceUtf8 = parseString(value.sourceUtf8, `${field}.sourceUtf8`);
  const sourceSha256 = parseString(value.sourceSha256, `${field}.sourceSha256`);
  const sourceStartByte = parseNonNegativeInteger(value.sourceStartByte, `${field}.sourceStartByte`);
  const sourceEndByte = parseNonNegativeInteger(value.sourceEndByte, `${field}.sourceEndByte`);
  if (
    (expectedOrdinal !== null && ordinal !== expectedOrdinal) ||
    !/^diagram-\d+-[a-f0-9]{12}$/.test(diagramId) ||
    !/^[a-f0-9]{64}$/.test(sourceSha256) ||
    sourceEndByte < sourceStartByte
  ) {
    throw contractError(field);
  }
  return { diagramId, ordinal, sourceUtf8, sourceSha256, sourceStartByte, sourceEndByte };
}

function parseSourceBlock(value: unknown, field: string): SourceBlock {
  if (!isRecord(value)) throw contractError(field);
  const startUtf16 = parseNonNegativeInteger(value.startUtf16, `${field}.startUtf16`);
  const endUtf16 = parseNonNegativeInteger(value.endUtf16, `${field}.endUtf16`);
  const startLine = parsePositiveInteger(value.startLine, `${field}.startLine`);
  const endLine = parsePositiveInteger(value.endLine, `${field}.endLine`);
  if (endUtf16 < startUtf16 || endLine < startLine) throw contractError(field);
  return { startUtf16, endUtf16, startLine, endLine };
}

export function parseMarkdownAnalysisDto(value: unknown): MarkdownAnalysisDto {
  if (!isRecord(value)) throw contractError('analyze_markdown');
  if (!Array.isArray(value.outline)) throw contractError('analyze_markdown.outline');
  return {
    outline: value.outline.map((item: unknown, index: number) => parseOutlineItem(item, `analyze_markdown.outline[${index}]`)),
    stats: parseDocumentStats(value.stats, 'analyze_markdown.stats')
  };
}

export function decodeLocalImageResponse(
  value: unknown,
  createObjectUrl: (blob: Blob) => string = (blob) => URL.createObjectURL(blob)
): LocalImageBatchDto {
  const bytes = normalizeBinaryResponse(value);
  if (bytes.byteLength < 5) throw contractError('load_local_image.body');
  const metadataLength = new DataView(bytes.buffer, bytes.byteOffset, 4).getUint32(0, true);
  if (
    metadataLength === 0 ||
    metadataLength > MAX_LOCAL_IMAGE_METADATA_BYTES ||
    4 + metadataLength > bytes.byteLength
  ) {
    throw contractError('load_local_image.metadataLength');
  }
  let metadata: unknown;
  try {
    metadata = JSON.parse(
      new TextDecoder('utf-8', { fatal: true }).decode(bytes.subarray(4, 4 + metadataLength))
    );
  } catch {
    throw contractError('load_local_image.metadata');
  }
  if (!isRecord(metadata) || !Array.isArray(metadata.entries) || !Array.isArray(metadata.resources)) {
    throw contractError('load_local_image.metadata');
  }
  if (
    metadata.entries.length > MAX_LOCAL_IMAGE_REFERENCES ||
    metadata.resources.length > MAX_LOCAL_IMAGE_REFERENCES
  ) {
    throw contractError('load_local_image.entries');
  }
  const payload = bytes.subarray(4 + metadataLength);
  if (payload.byteLength > MAX_LOCAL_IMAGE_PAYLOAD_BYTES) {
    throw contractError('load_local_image.payload');
  }
  let expectedOffset = 0;
  let decodedTotal = 0;
  const resourceDescriptors = metadata.resources.map((raw, index) => {
    if (!isRecord(raw)) throw contractError(`load_local_image.resources[${index}]`);
    const mime = String(raw.mime);
    const path = raw.path;
    const width = parsePositiveInteger(raw.width, `load_local_image.resources[${index}].width`);
    const height = parsePositiveInteger(raw.height, `load_local_image.resources[${index}].height`);
    const encodedBytes = parsePositiveInteger(raw.encodedBytes, `load_local_image.resources[${index}].encodedBytes`);
    const decodedBytes = parsePositiveInteger(raw.decodedBytes, `load_local_image.resources[${index}].decodedBytes`);
    const offset = parseNonNegativeInteger(raw.offset, `load_local_image.resources[${index}].offset`);
    if (
      !['image/png', 'image/jpeg', 'image/gif', 'image/webp'].includes(mime) ||
      typeof path !== 'string' ||
      offset !== expectedOffset ||
      offset + encodedBytes > payload.byteLength ||
      decodedBytes !== width * height * 4
    ) {
      throw contractError(`load_local_image.resources[${index}]`);
    }
    expectedOffset += encodedBytes;
    decodedTotal += decodedBytes;
    if (decodedTotal > MAX_LOCAL_IMAGE_DECODED_BYTES) throw contractError('load_local_image.decodedBudget');
    const resourceBytes = new Uint8Array(encodedBytes);
    resourceBytes.set(payload.subarray(offset, offset + encodedBytes));
    return { blob: new Blob([resourceBytes], { type: mime }), path, width, height, encodedBytes, decodedBytes };
  });
  if (expectedOffset !== payload.byteLength) throw contractError('load_local_image.payload');
  const seenTargets = new Set<string>();
  const entryDescriptors = metadata.entries.map((raw, index) => {
    if (!isRecord(raw) || typeof raw.target !== 'string') {
      throw contractError(`load_local_image.entries[${index}]`);
    }
    if (seenTargets.has(raw.target)) throw contractError(`load_local_image.entries[${index}].target`);
    seenTargets.add(raw.target);
    const resourceIndex = raw.resourceIndex;
    const error = raw.error === null ? null : parseAppError(raw.error, `load_local_image.entries[${index}].error`);
    if (resourceIndex === null) {
      if (!error) throw contractError(`load_local_image.entries[${index}]`);
      return { target: raw.target, resourceIndex: null, error };
    }
    const parsedIndex = parseNonNegativeInteger(resourceIndex, `load_local_image.entries[${index}].resourceIndex`);
    if (parsedIndex >= resourceDescriptors.length || error) throw contractError(`load_local_image.entries[${index}]`);
    return { target: raw.target, resourceIndex: parsedIndex, error: null };
  });
  const objectUrls: string[] = [];
  let resources;
  try {
    resources = resourceDescriptors.map((resource) => {
      const objectUrl = createObjectUrl(resource.blob);
      objectUrls.push(objectUrl);
      const { blob: _blob, ...metadata } = resource;
      return { objectUrl, ...metadata };
    });
  } catch (error) {
    for (const objectUrl of objectUrls) {
      if (objectUrl.startsWith('blob:')) URL.revokeObjectURL(objectUrl);
    }
    throw error;
  }
  const entries = entryDescriptors.map((entry) => ({
    target: entry.target,
    resource: entry.resourceIndex === null ? null : resources[entry.resourceIndex],
    error: entry.error
  }));
  return { entries, objectUrls };
}

const MAX_LOCAL_IMAGE_REFERENCES = 256;
const MAX_LOCAL_IMAGE_METADATA_BYTES = 4 * 1024 * 1024;
const MAX_LOCAL_IMAGE_PAYLOAD_BYTES = 32 * 1024 * 1024;
const MAX_LOCAL_IMAGE_DECODED_BYTES = 128 * 1024 * 1024;
const MAX_LOCAL_IMAGE_RESPONSE_BYTES =
  4 + MAX_LOCAL_IMAGE_METADATA_BYTES + MAX_LOCAL_IMAGE_PAYLOAD_BYTES;

function normalizeBinaryResponse(value: unknown): Uint8Array {
  let bytes: Uint8Array;
  if (ArrayBuffer.isView(value)) {
    bytes = new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
  } else if (
    value instanceof ArrayBuffer ||
    Object.prototype.toString.call(value) === '[object ArrayBuffer]'
  ) {
    bytes = new Uint8Array(value as ArrayBuffer);
  } else if (Array.isArray(value)) {
    if (value.length > MAX_LOCAL_IMAGE_RESPONSE_BYTES) {
      throw contractError('load_local_image.body');
    }
    bytes = new Uint8Array(value.length);
    for (let index = 0; index < value.length; index += 1) {
      const byte = value[index];
      if (!(index in value) || !Number.isInteger(byte) || byte < 0 || byte > 255) {
        throw contractError('load_local_image.body');
      }
      bytes[index] = byte;
    }
  } else {
    throw contractError('load_local_image.body');
  }
  if (bytes.byteLength > MAX_LOCAL_IMAGE_RESPONSE_BYTES) {
    throw contractError('load_local_image.body');
  }
  return bytes;
}

export function parseOptionalString(value: unknown, field: string): string | null {
  if (value === null || typeof value === 'string') return value;
  throw contractError(field);
}

export function parseStringArray(value: unknown, field: string): string[] {
  if (!Array.isArray(value) || !value.every((item) => typeof item === 'string')) {
    throw contractError(field);
  }
  return [...value];
}

export function parseAppSettings(value: unknown): AppSettings {
  if (!isRecord(value)) throw contractError('settings');
  const language = parseEnum(value.language, ['en', 'zh-CN'], 'settings.language');
  const theme = parseEnum(value.theme, ['light', 'dark', 'system'], 'settings.theme') as ThemeMode;
  return {
    language,
    theme,
    accentColor: parseString(value.accentColor, 'settings.accentColor'),
    editorFontFamily: parseString(value.editorFontFamily, 'settings.editorFontFamily'),
    previewFontFamily: parseString(value.previewFontFamily, 'settings.previewFontFamily'),
    editorFontSize: parseFiniteNumber(value.editorFontSize, 'settings.editorFontSize'),
    previewFontSize: parseFiniteNumber(value.previewFontSize, 'settings.previewFontSize'),
    lineHeight: parseFiniteNumber(value.lineHeight, 'settings.lineHeight'),
    cornerRadius: parseFiniteNumber(value.cornerRadius, 'settings.cornerRadius'),
    showLineNumbers: parseBoolean(value.showLineNumbers, 'settings.showLineNumbers'),
    wordWrap: parseBoolean(value.wordWrap, 'settings.wordWrap'),
    tabSize: parseFiniteNumber(value.tabSize, 'settings.tabSize'),
    insertSpaces: parseBoolean(value.insertSpaces, 'settings.insertSpaces'),
    autosaveEnabled: parseBoolean(value.autosaveEnabled, 'settings.autosaveEnabled'),
    autosaveIntervalMs: parseFiniteNumber(value.autosaveIntervalMs, 'settings.autosaveIntervalMs'),
    livePreviewEnabled: parseBoolean(value.livePreviewEnabled, 'settings.livePreviewEnabled'),
    previewDebounceMs: parseFiniteNumber(value.previewDebounceMs, 'settings.previewDebounceMs'),
    syncScroll: parseBoolean(value.syncScroll, 'settings.syncScroll'),
    showSidebar: parseBoolean(value.showSidebar, 'settings.showSidebar'),
    showStatusBar: parseBoolean(value.showStatusBar, 'settings.showStatusBar'),
    restoreLastSession: parseBoolean(value.restoreLastSession, 'settings.restoreLastSession'),
    recentFilesLimit: parseFiniteNumber(value.recentFilesLimit, 'settings.recentFilesLimit'),
    markdownToolbarEnabled: parseBoolean(value.markdownToolbarEnabled, 'settings.markdownToolbarEnabled'),
    allowLocalImages: parseBoolean(value.allowLocalImages, 'settings.allowLocalImages'),
    confirmExternalLinks: parseBoolean(value.confirmExternalLinks, 'settings.confirmExternalLinks')
  };
}

export function parseRecentFiles(value: unknown): RecentFileDto[] {
  if (!Array.isArray(value)) throw contractError('recent_files');
  return value.map((item, index) => {
    if (!isRecord(item)) throw contractError(`recent_files[${index}]`);
    return {
      path: parseString(item.path, `recent_files[${index}].path`),
      title: parseString(item.title, `recent_files[${index}].title`),
      lastOpenedAt: parseString(item.lastOpenedAt, `recent_files[${index}].lastOpenedAt`)
    };
  });
}

export function parseSessionState(value: unknown): SessionStateDto {
  if (!isRecord(value) || value.version !== 1) throw contractError('session.version');
  return {
    version: 1,
    paths: parseStringArray(value.paths, 'session.paths'),
    activePath: parseOptionalString(value.activePath, 'session.activePath')
  };
}

export function parseMarkdownTarget(value: unknown): MarkdownTargetDto {
  if (!isRecord(value)) throw contractError('markdown_target');
  if (value.kind === 'anchor') {
    return { kind: 'anchor', fragment: parseString(value.fragment, 'markdown_target.fragment') };
  }
  if (value.kind === 'localDocument') {
    return {
      kind: 'localDocument',
      path: parseString(value.path, 'markdown_target.path'),
      fragment: parseOptionalString(value.fragment, 'markdown_target.fragment')
    };
  }
  if (value.kind === 'external') {
    return { kind: 'external', url: parseString(value.url, 'markdown_target.url') };
  }
  if (value.kind === 'email') {
    return { kind: 'email', address: parseString(value.address, 'markdown_target.address') };
  }
  throw contractError('markdown_target.kind');
}

export function parseExportResult(value: unknown): ExportResult {
  if (!isRecord(value)) throw contractError('export_result');
  if (!Array.isArray(value.warnings)) throw contractError('export_result.warnings');
  const format = parseEnum(value.format, ['html', 'pdf', 'docx', 'svg', 'png'], 'export_result.format') as ExportFormat;
  const targetKind = value.targetKind === undefined ? 'file' : parseEnum(value.targetKind, ['file', 'directory'], 'export_result.targetKind');
  if ((format === 'png') !== (targetKind === 'directory')) throw contractError('export_result.targetKind');
  return {
    jobId: parseString(value.jobId, 'export_result.jobId'),
    format,
    ...(value.targetKind === undefined ? {} : { targetKind }),
    path: parseString(value.path, 'export_result.path'),
    warnings: value.warnings.map((warning, index) => parseExportWarning(warning, index))
  };
}

export function parseStartupReady(value: unknown): StartupReadyDto {
  if (!isRecord(value)) throw contractError('startup_ready');
  return {
    launchId: parseString(value.launchId, 'startup_ready.launchId'),
    webviewVersion: parseOptionalString(value.webviewVersion, 'startup_ready.webviewVersion')
  };
}

export function parseStartupDiagnosticsExport(value: unknown): StartupDiagnosticsExportDto {
  if (!isRecord(value)) throw contractError('startup_diagnostics_export');
  return {
    recordCount: parseNonNegativeInteger(value.recordCount, 'startup_diagnostics_export.recordCount')
  };
}

function parseDocumentDto(value: unknown): DocumentDto {
  if (!isRecord(value)) throw contractError('document_operation.document');
  return {
    path: parseOptionalString(value.path, 'document.path'),
    fileIdentity: parseOptionalString(value.fileIdentity, 'document.fileIdentity'),
    contentVersion: parseOptionalString(value.contentVersion, 'document.contentVersion'),
    title: parseString(value.title, 'document.title'),
    content: parseString(value.content, 'document.content'),
    isDirty: parseBoolean(value.isDirty, 'document.isDirty'),
    lastSavedAt: parseOptionalString(value.lastSavedAt, 'document.lastSavedAt'),
    fileSize: value.fileSize === null ? null : parseNonNegativeInteger(value.fileSize, 'document.fileSize')
  };
}

function parseOutlineItem(value: unknown, field: string): OutlineItem {
  if (!isRecord(value)) throw contractError(field);
  return {
    level: parseIntegerInRange(value.level, 1, 6, `${field}.level`),
    title: parseString(value.title, `${field}.title`),
    line: parseIntegerInRange(value.line, 1, Number.MAX_SAFE_INTEGER, `${field}.line`),
    slug: parseString(value.slug, `${field}.slug`)
  };
}

function parseDocumentStats(value: unknown, field: string): DocumentStats {
  if (!isRecord(value)) throw contractError(field);
  return {
    wordCount: parseNonNegativeInteger(value.wordCount, `${field}.wordCount`),
    characterCount: parseNonNegativeInteger(value.characterCount, `${field}.characterCount`),
    lineCount: parseNonNegativeInteger(value.lineCount, `${field}.lineCount`),
    headingCount: parseNonNegativeInteger(value.headingCount, `${field}.headingCount`),
    linkCount: parseNonNegativeInteger(value.linkCount, `${field}.linkCount`),
    imageCount: parseNonNegativeInteger(value.imageCount, `${field}.imageCount`)
  };
}

function parseExportWarning(value: unknown, index: number): ExportWarning {
  const field = `export_result.warnings[${index}]`;
  if (!isRecord(value)) throw contractError(field);
  return {
    code: parseString(value.code, `${field}.code`),
    message: parseString(value.message, `${field}.message`),
    target: parseOptionalString(value.target, `${field}.target`)
  };
}

function parseAppError(value: unknown, field: string): AppError {
  if (!isRecord(value)) throw contractError(field);
  return {
    code: parseString(value.code, `${field}.code`),
    message: parseString(value.message, `${field}.message`)
  };
}

export function parseString(value: unknown, field: string): string {
  if (typeof value === 'string') return value;
  throw contractError(field);
}

export function parseBoolean(value: unknown, field: string): boolean {
  if (typeof value === 'boolean') return value;
  throw contractError(field);
}

function parseFiniteNumber(value: unknown, field: string): number {
  if (typeof value === 'number' && Number.isFinite(value)) return value;
  throw contractError(field);
}

function parseNonNegativeInteger(value: unknown, field: string): number {
  return parseIntegerInRange(value, 0, Number.MAX_SAFE_INTEGER, field);
}

function parsePositiveInteger(value: unknown, field: string): number {
  return parseIntegerInRange(value, 1, Number.MAX_SAFE_INTEGER, field);
}

function parseIntegerInRange(value: unknown, minimum: number, maximum: number, field: string): number {
  if (Number.isInteger(value) && Number(value) >= minimum && Number(value) <= maximum) {
    return Number(value);
  }
  throw contractError(field);
}

function parseEnum<const Value extends string>(
  value: unknown,
  allowed: readonly Value[],
  field: string
): Value {
  if (typeof value === 'string' && allowed.includes(value as Value)) return value as Value;
  throw contractError(field);
}

function contractError(field: string): Error & AppError {
  return Object.assign(new Error(`The desktop protocol returned an invalid field: ${field}`), {
    code: 'INVALID_DESKTOP_CONTRACT'
  });
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}
