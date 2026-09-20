import { describe, expect, test, vi } from 'vitest';
import fixtures from '../../shared/desktop-contract-fixtures.json';
import { defaultSettings, type ExportRequest } from './contracts';
import type { CommandTransport } from './runtime';
import { createTauriClient } from './tauriClient';

const documentResponse = {
  document: {
    path: 'C:\\docs\\note.md',
    fileIdentity: 'file-id',
    contentVersion: 'sha256:note',
    title: 'note.md',
    content: '# Note',
    isDirty: false,
    lastSavedAt: '2026-08-13T12:00:00Z',
    fileSize: 6
  },
  auxiliaryError: null
};

function clientWithResponse(response: unknown) {
  const invoke = vi.fn(async () => response);
  const transport: CommandTransport = { invoke };
  return { client: createTauriClient(transport), invoke };
}

function localImageEnvelope(): Uint8Array {
  return Uint8Array.from(atob(fixtures.localImageResponse.responseBase64), (character) =>
    character.charCodeAt(0)
  );
}

describe('validated Tauri command client', () => {
  test('validates the shared virtual preview fixture and its versioned commands', async () => {
    const invoke = vi.fn(async (command: string) => {
      if (command === 'render_markdown') return {
        ...fixtures.renderedMarkdown,
        html: '', sourceBlocks: [], diagrams: [], diagramDiagnostics: [],
        virtualPreview: fixtures.virtualPreviewIndex
      };
      if (command === 'render_markdown_window') return fixtures.virtualPreviewWindow;
      return undefined;
    });
    const client = createTauriClient({ invoke });
    await expect(client.renderMarkdown('large source', 'tab-7', 4)).resolves.toMatchObject({
      virtualPreview: fixtures.virtualPreviewIndex
    });
    await expect(client.renderMarkdownWindow('session-7', 0, 1)).resolves.toEqual(fixtures.virtualPreviewWindow);
    await client.releaseMarkdownPreview('session-7');
    expect(invoke.mock.calls).toEqual([
      ['render_markdown', { content: 'large source', tabId: 'tab-7', contentRevision: 4 }],
      ['render_markdown_window', { sessionId: 'session-7', start: 0, end: 1 }],
      ['release_markdown_preview', { sessionId: 'session-7' }]
    ]);
  });

  test('passes a typed email address to the native command wrapper', async () => {
    const invoke = vi.fn(async () => undefined);
    await createTauriClient({ invoke }).openValidatedEmailLink('writer@example.org');
    expect(invoke).toHaveBeenCalledWith('open_validated_email_link', {
      address: 'writer@example.org'
    });
  });

  test('sends the complete save CAS contract and validates the document response', async () => {
    const { client, invoke } = clientWithResponse(documentResponse);

    await expect(
      client.saveMarkdownFile('C:\\docs\\note.md', '# Note', 'file-id', 'sha256:note', false)
    ).resolves.toEqual(documentResponse);
    expect(invoke).toHaveBeenCalledWith('save_markdown_file', {
      path: 'C:\\docs\\note.md',
      content: '# Note',
      expectedFileIdentity: 'file-id',
      expectedContentVersion: 'sha256:note',
      overwriteContentConflict: false
    });
  });

  test('keeps command argument names stable for navigation and file identity', async () => {
    const responses = new Map<string, unknown>([
      ['resolve_markdown_target', { kind: 'localDocument', path: 'C:\\docs\\other.md', fragment: 'part' }],
      ['resolve_file_identity', 'identity-2'],
      ['resolve_file_version', { fileIdentity: 'identity-2', contentVersion: 'sha256:other' }]
    ]);
    const invoke = vi.fn(async (command: string) => responses.get(command));
    const client = createTauriClient({ invoke });

    await expect(client.resolveMarkdownTarget('C:\\docs\\note.md', './other.md#part')).resolves.toEqual({
      kind: 'localDocument',
      path: 'C:\\docs\\other.md',
      fragment: 'part'
    });
    await expect(client.resolveFileIdentity('C:\\docs\\other.md', true)).resolves.toBe('identity-2');
    await expect(client.resolveFileVersion('C:\\docs\\other.md', true)).resolves.toEqual({
      fileIdentity: 'identity-2',
      contentVersion: 'sha256:other'
    });
    expect(invoke).toHaveBeenNthCalledWith(1, 'resolve_markdown_target', {
      documentPath: 'C:\\docs\\note.md',
      target: './other.md#part'
    });
    expect(invoke).toHaveBeenNthCalledWith(2, 'resolve_file_identity', {
      path: 'C:\\docs\\other.md',
      allowMissing: true
    });
    expect(invoke).toHaveBeenNthCalledWith(3, 'resolve_file_version', {
      path: 'C:\\docs\\other.md',
      allowMissing: true
    });
  });

  test('rejects a malformed response instead of trusting a generic cast', async () => {
    const { client } = clientWithResponse({ ...documentResponse, document: { title: 'missing fields' } });

    await expect(client.openMarkdownFile('C:\\docs\\bad.md')).rejects.toMatchObject({
      code: 'INVALID_DESKTOP_CONTRACT'
    });
  });

  test('validates settings and session responses from an injected transport', async () => {
    const invoke = vi.fn(async (command: string) => {
      if (command === 'get_settings') return { theme: 'system' };
      return { version: 2, paths: [], activePath: null };
    });
    const client = createTauriClient({ invoke });

    await expect(client.getSettings()).rejects.toMatchObject({ code: 'INVALID_DESKTOP_CONTRACT' });
    await expect(client.getSession()).rejects.toMatchObject({ code: 'INVALID_DESKTOP_CONTRACT' });
  });

  test('maps every remaining facade method to its stable desktop command and argument object', async () => {
    const responses: Record<string, unknown> = {
      open_markdown_file: documentResponse,
      suggest_export_path: {path:'C:\\last\\note.html',warning:null},
      export_document: {
        jobId: 'export-1',
        format: 'html',
        path: 'C:\\out\\note.html',
        warnings: []
      },
      get_startup_file_arg: null,
      render_markdown: {
        html: '<h1>Note</h1>',
        sourceBlocks: [{ startUtf16: 0, endUtf16: 6, startLine: 1, endLine: 1 }],
        diagrams: [],
        diagramDiagnostics: [],
        outline: [{ level: 1, title: 'Note', line: 1, slug: 'note' }],
        stats: {
          wordCount: 1,
          characterCount: 6,
          lineCount: 1,
          headingCount: 1,
          linkCount: 0,
          imageCount: 0
        }
      },
      validate_diagram_svg: {
        diagramId: 'diagram-0-aaaaaaaaaaaa',
        sourceSha256: 'a'.repeat(64),
        cacheKey: 'b'.repeat(64),
        rendererId: 'mermaid-offline-11.17.2',
        svgUtf8: '<svg viewBox="0 0 100 50"></svg>',
        width: 100,
        height: 50,
        viewBox: [0, 0, 100, 50],
        accessibleTitle: null,
        accessibleDescription: null,
        warnings: []
      },
      analyze_markdown: {
        outline: [{ level: 1, title: 'Note', line: 1, slug: 'note' }],
        stats: {
          wordCount: 1,
          characterCount: 6,
          lineCount: 1,
          headingCount: 1,
          linkCount: 0,
          imageCount: 0
        }
      },
      load_local_image: Array.from(localImageEnvelope()),
      drain_open_file_requests: ['C:\\docs\\queued.md'],
      get_settings: defaultSettings,
      update_settings: defaultSettings,
      reset_settings: defaultSettings,
      get_recent_files: [],
      remove_recent_file: [],
      clear_missing_recent_files: [],
      get_session: { version: 1, paths: [], activePath: null },
      update_session: { version: 1, paths: [], activePath: null },
      mark_frontend_ready: { launchId: 'launch-1', webviewVersion: null },
      export_startup_diagnostics: { recordCount: 2 }
    };
    const invoke = vi.fn(async (command: string) => responses[command]);
    const client = createTauriClient({ invoke }, () => 'blob:local-image');
    const exportRequest: ExportRequest = {
      snapshot: {
        jobId: 'export-1',
        tabId: 'tab-1',
        contentRevision: 2,
        sourcePath: 'C:\\docs\\note.md',
        title: 'note.md',
        content: '# Note'
      },
      targetPath: 'C:\\out\\note.html',
      format: 'html',
      mindMapSvg: null,
      options: {
        paperSize: 'a4',
        orientation: 'portrait',
        margin: 'normal',
        includeTitle: true,
        includeLocalImages: false
      }
    };
    const event = {
      stage: 'settings' as const,
      status: 'started' as const,
      elapsedMs: 12,
      code: null
    };

    await client.openMarkdownFile('C:\\docs\\note.md');
    await client.exportDocument(exportRequest);
    await client.suggestExportPath('C:\\docs\\note.html');
    await client.rememberExportDirectory('C:\\last\\note.html');
    await client.getStartupFileArg();
    await client.renderMarkdown('# Note');
    await client.validateDiagramSvg({
      diagramId: 'diagram-0-aaaaaaaaaaaa',
      sourceSha256: 'a'.repeat(64),
      cacheKey: 'b'.repeat(64),
      rendererId: 'mermaid-offline-11.17.2',
      svgUtf8: '<svg viewBox="0 0 100 50"></svg>',
      width: 100,
      height: 50,
      viewBox: [0, 0, 100, 50],
      accessibleTitle: null,
      accessibleDescription: null,
      warnings: []
    });
    await client.analyzeMarkdown('# Note');
    await client.loadLocalImages('C:\\docs\\note.md', ['./image.png'], 'image-job');
    await client.cancelLocalImageJob('image-job');
    await client.drainOpenFileRequests();
    await client.getSettings();
    await client.updateSettings(defaultSettings);
    await client.resetSettings();
    await client.getRecentFiles();
    await client.removeRecentFile('C:\\docs\\note.md');
    await client.clearMissingRecentFiles();
    await client.getSession();
    await client.updateSession({ version: 1, paths: [], activePath: null });
    await client.clearSession();
    await client.showInFileManager('C:\\docs\\note.md');
    await client.recordFrontendStartupEvent(event);
    await client.markFrontendReady(42);
    await client.clearStartupDiagnostics();
    await client.exportStartupDiagnostics('C:\\out\\diagnostics.json');

    expect(invoke.mock.calls).toEqual([
      ['open_markdown_file', { path: 'C:\\docs\\note.md' }],
      ['export_document', { request: exportRequest }],
      ['suggest_export_path', {defaultPath:'C:\\docs\\note.html'}],
      ['remember_export_directory', {targetPath:'C:\\last\\note.html'}],
      ['get_startup_file_arg', undefined],
      ['render_markdown', { content: '# Note' }],
      [
        'validate_diagram_svg',
        {
          diagram: {
            diagramId: 'diagram-0-aaaaaaaaaaaa',
            sourceSha256: 'a'.repeat(64),
            cacheKey: 'b'.repeat(64),
            rendererId: 'mermaid-offline-11.17.2',
            svgUtf8: '<svg viewBox="0 0 100 50"></svg>',
            width: 100,
            height: 50,
            viewBox: [0, 0, 100, 50],
            accessibleTitle: null,
            accessibleDescription: null,
            warnings: []
          }
        }
      ],
      ['analyze_markdown', { content: '# Note' }],
      ['load_local_image', { documentPath: 'C:\\docs\\note.md', targets: ['./image.png'], jobId: 'image-job' }],
      ['cancel_local_image_job', { jobId: 'image-job' }],
      ['drain_open_file_requests', undefined],
      ['get_settings', undefined],
      ['update_settings', { settings: defaultSettings }],
      ['reset_settings', undefined],
      ['get_recent_files', undefined],
      ['remove_recent_file', { path: 'C:\\docs\\note.md' }],
      ['clear_missing_recent_files', undefined],
      ['get_session', undefined],
      ['update_session', { session: { version: 1, paths: [], activePath: null } }],
      ['clear_session', undefined],
      ['show_in_file_manager', { path: 'C:\\docs\\note.md' }],
      ['record_frontend_startup_event', { event }],
      ['mark_frontend_ready', { elapsedMs: 42 }],
      ['clear_startup_diagnostics', undefined],
      ['export_startup_diagnostics', { path: 'C:\\out\\diagnostics.json' }]
    ]);
  });
});

test('PNG wrappers preserve directory identity and validate cancellation results', async () => {
  const {client, invoke} = clientWithResponse('C:/notes/book');
  await expect(client.resolvePngExportDirectory('C:/notes/book.md', 'book', null)).resolves.toBe('C:/notes/book');
  expect(invoke).toHaveBeenCalledWith('resolve_png_export_directory', {sourcePath:'C:/notes/book.md', title:'book', parentPath:null});
  await expect(client.cancelPngExport('job')).rejects.toBeDefined();
  const cancelled = clientWithResponse(true);
  await expect(cancelled.client.cancelPngExport('job')).resolves.toBe(true);
  expect(cancelled.invoke).toHaveBeenCalledWith('cancel_png_export', {jobId:'job'});
});
