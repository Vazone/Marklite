import { beforeEach, describe, expect, test, vi } from 'vitest';
import fixtures from '../shared/desktop-contract-fixtures.json';
import {
  api,
  decodeLocalImageResponse,
  defaultSettings,
  isTauriRuntime,
  openExternalLink,
  parseRenderedMarkdownDto
} from './tauriApi';

describe('browser fallback', () => {
  beforeEach(() => {
    delete (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
  });

  test('provides only explicit non-persistent state adapters without a desktop runtime', async () => {
    expect(isTauriRuntime()).toBe(false);
    await expect(api.getSettings()).resolves.toEqual(defaultSettings);
    await expect(api.getRecentFiles()).resolves.toEqual([]);

    await expect(api.renderMarkdown('# Title')).rejects.toThrow(
      'browser adapter does not emulate the desktop protocol'
    );
    await expect(api.analyzeMarkdown('# Title')).rejects.toThrow(
      'browser adapter does not emulate the desktop protocol'
    );
  });

  test('rejects native-only file writes instead of pretending they succeeded', async () => {
    await expect(api.saveMarkdownFile('C:\\docs\\a.md', 'content', null, null, false)).rejects.toThrow(
      'This operation requires the Tauri desktop runtime.'
    );
  });

  test('exposes a non-persistent session adapter without a desktop runtime', async () => {
    const session = {
      version: 1 as const,
      paths: ['C:\\docs\\a.md'],
      activePath: 'C:\\docs\\a.md'
    };

    await expect(api.getSession()).resolves.toEqual({ version: 1, paths: [], activePath: null });
    await expect(api.updateSession(session)).resolves.toEqual(session);
    await expect(api.clearSession()).resolves.toBeUndefined();
  });

  test('rejects non-http external schemes before reaching the opener', async () => {
    await expect(openExternalLink('javascript:alert(1)')).rejects.toMatchObject({
      code: 'UNSUPPORTED_LINK_SCHEME'
    });
  });

  test('brands only structurally valid sanitized-renderer responses', () => {
    expect(
      parseRenderedMarkdownDto({
        html: '<h1 id="title">Title</h1>',
        outline: [{ level: 1, title: 'Title', line: 1, slug: 'title' }],
        sourceBlocks: [{ startUtf16: 0, endUtf16: 7, startLine: 1, endLine: 1 }],
        diagrams: [],
        diagramDiagnostics: [],
        stats: {
          wordCount: 1,
          characterCount: 7,
          lineCount: 1,
          headingCount: 1,
          linkCount: 0,
          imageCount: 0
        }
      }).html
    ).toContain('id="title"');

    expect(() =>
      parseRenderedMarkdownDto({ html: '<p>bad</p>', outline: [], stats: { lineCount: -1 } })
    ).toThrowError(expect.objectContaining({ code: 'INVALID_DESKTOP_CONTRACT' }));
  });

  test('decodes the binary local-image envelope without base64 materialization', () => {
    const body = Uint8Array.from(atob(fixtures.localImageResponse.responseBase64), (character) =>
      character.charCodeAt(0)
    );
    const createObjectUrl = vi.fn((_blob: Blob) => 'blob:marklite-image');

    expect(decodeLocalImageResponse(body, createObjectUrl)).toEqual({
      entries: [{
        target: fixtures.localImageResponse.metadata.entries[0].target,
        resource: {
          objectUrl: 'blob:marklite-image',
          path: fixtures.localImageResponse.metadata.resources[0].path,
          width: 1,
          height: 1,
          encodedBytes: 8,
          decodedBytes: 4
        },
        error: null
      }],
      objectUrls: ['blob:marklite-image']
    });
    expect(createObjectUrl.mock.calls[0][0]).toBeInstanceOf(Blob);
  });

  test('decodes the strict byte-array shape used by the Tauri postMessage fallback', async () => {
    const body = Uint8Array.from(atob(fixtures.localImageResponse.responseBase64), (character) =>
      character.charCodeAt(0)
    );
    let receivedBlob: Blob | undefined;

    expect(
      decodeLocalImageResponse(Array.from(body), (blob) => {
        receivedBlob = blob;
        return 'blob:post-message-image';
      })
    ).toEqual({
      entries: [{
        target: fixtures.localImageResponse.metadata.entries[0].target,
        resource: {
          objectUrl: 'blob:post-message-image',
          path: fixtures.localImageResponse.metadata.resources[0].path,
          width: 1,
          height: 1,
          encodedBytes: 8,
          decodedBytes: 4
        },
        error: null
      }],
      objectUrls: ['blob:post-message-image']
    });
    await expect(receivedBlob?.arrayBuffer()).resolves.toEqual(
      Uint8Array.from(atob(fixtures.localImageResponse.payloadBase64), (character) =>
        character.charCodeAt(0)
      ).buffer
    );
  });

  test('decodes ArrayBuffer and non-Uint8 ArrayBuffer views from the custom IPC channel', () => {
    const body = Uint8Array.from(atob(fixtures.localImageResponse.responseBase64), (character) =>
      character.charCodeAt(0)
    );
    const arrayBuffer = body.buffer.slice(body.byteOffset, body.byteOffset + body.byteLength);

    expect(decodeLocalImageResponse(arrayBuffer, () => 'blob:array-buffer').objectUrls[0]).toBe(
      'blob:array-buffer'
    );
    expect(
      decodeLocalImageResponse(new DataView(arrayBuffer), () => 'blob:data-view').objectUrls[0]
    ).toBe('blob:data-view');
  });

  test('rejects malformed or unapproved binary image metadata', () => {
    const bad = new Uint8Array([1, 0, 0, 0, 123, 1]);
    expect(() => decodeLocalImageResponse(bad, () => 'blob:bad')).toThrowError(
      expect.objectContaining({ code: 'INVALID_DESKTOP_CONTRACT' })
    );
  });

  test('rejects malformed or unbounded postMessage byte arrays', () => {
    for (const invalid of [[0, -1], [0, 256], [0, 1.5], [0, '1']]) {
      expect(() => decodeLocalImageResponse(invalid, () => 'blob:bad')).toThrow(
        'load_local_image.body'
      );
    }
    const sparse = new Array<number>(6);
    sparse[0] = 1;
    expect(() => decodeLocalImageResponse(sparse, () => 'blob:bad')).toThrow(
      'load_local_image.body'
    );
    expect(() =>
      decodeLocalImageResponse(new Uint8Array(32 * 1024 * 1024 + 4 * 1024 * 1024 + 5), () => 'blob:bad')
    ).toThrow('load_local_image.body');
  });
});
