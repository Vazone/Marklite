import { describe, expect, test } from 'vitest';
import fixtures from '../../shared/desktop-contract-fixtures.json';
import {
  decodeLocalImageResponse,
  parseDocumentOperationDto,
  parseExportResult,
  parseExportPathSuggestion,
  parseFileVersionDto,
  parseMarkdownTarget,
  parseMarkdownAnalysisDto,
  parseRecentFiles,
  parseRenderedMarkdownDto,
  parseSessionState,
  parseStartupDiagnosticsExport,
  parseStartupReady,
  toAppError
} from './contractValidation';

describe('shared Rust/TypeScript desktop contract fixtures', () => {
  test('parses document operations including identity, nulls and typed auxiliary errors', () => {
    expect(parseDocumentOperationDto(fixtures.documentOperation)).toEqual(
      fixtures.documentOperation
    );
    expect(
      parseDocumentOperationDto({
        document: fixtures.untitledDocument,
        auxiliaryError: null
      })
    ).toEqual({ document: fixtures.untitledDocument, auxiliaryError: null });
    expect(parseFileVersionDto(fixtures.fileVersion)).toEqual(fixtures.fileVersion);
  });

  test('parses recent, session, startup and error fixtures without field rewriting', () => {
    expect(parseRecentFiles(fixtures.recentResponse)).toEqual(fixtures.recentResponse);
    expect(parseSessionState(fixtures.session)).toEqual(fixtures.session);
    expect(parseStartupReady(fixtures.startupReady)).toEqual(fixtures.startupReady);
    expect(toAppError(fixtures.appError)).toEqual(fixtures.appError);
    expect(fixtures.recentPersistence).toEqual({
      version: 1,
      files: fixtures.recentResponse
    });
  });

  test('fails when high-risk camelCase fields drift', () => {
    expect(() =>
      parseDocumentOperationDto({
        ...fixtures.documentOperation,
        document: {
          ...fixtures.documentOperation.document,
          fileIdentity: undefined,
          file_identity: fixtures.documentOperation.document.fileIdentity
        }
      })
    ).toThrow(/document\.fileIdentity/);
    expect(() => parseSessionState({ ...fixtures.session, version: 2 })).toThrow(
      /session\.version/
    );
  });

  test('parses render, navigation, export and diagnostics fixtures shared with Rust', () => {
    expect(parseRenderedMarkdownDto(fixtures.renderedMarkdown)).toEqual(
      fixtures.renderedMarkdown
    );
    expect(parseMarkdownAnalysisDto(fixtures.markdownAnalysis)).toEqual(fixtures.markdownAnalysis);
    expect(fixtures.markdownTargets.map(parseMarkdownTarget)).toEqual(fixtures.markdownTargets);
    expect(parseExportResult(fixtures.exportResult)).toEqual(fixtures.exportResult);
    expect(parseExportPathSuggestion(fixtures.exportPathSuggestion)).toEqual(fixtures.exportPathSuggestion);
    expect(parseExportPathSuggestion({...fixtures.exportPathSuggestion,warning:null}).warning).toBeNull();
    expect(() => parseExportPathSuggestion({path:123,warning:null})).toThrow();
    expect(() => parseExportPathSuggestion({path:'out.html'})).toThrow();
    expect(parseStartupDiagnosticsExport(fixtures.startupDiagnosticsExport)).toEqual(
      fixtures.startupDiagnosticsExport
    );
  });

  test('decodes the exact local-image envelope emitted by the Rust production encoder', async () => {
    const response = Uint8Array.from(atob(fixtures.localImageResponse.responseBase64), (character) =>
      character.charCodeAt(0)
    );
    const payload = Uint8Array.from(atob(fixtures.localImageResponse.payloadBase64), (character) =>
      character.charCodeAt(0)
    );
    let receivedBlob: Blob | undefined;

    expect(
      decodeLocalImageResponse(response, (blob) => {
        receivedBlob = blob;
        return 'blob:shared-contract';
      })
    ).toEqual({
      entries: [{
        target: fixtures.localImageResponse.metadata.entries[0].target,
        resource: {
          objectUrl: 'blob:shared-contract',
          path: fixtures.localImageResponse.metadata.resources[0].path,
          width: 1,
          height: 1,
          encodedBytes: 8,
          decodedBytes: 4
        },
        error: null
      }],
      objectUrls: ['blob:shared-contract']
    });
    expect(receivedBlob?.type).toBe(fixtures.localImageResponse.metadata.resources[0].mime);
    expect(receivedBlob?.size).toBe(payload.byteLength);
    await expect(receivedBlob?.arrayBuffer()).resolves.toEqual(payload.buffer);
  });
});
