import { describe, expect, test, vi } from 'vitest';
import { applyDocumentOperation } from './documentOperation';
import type { DocumentOperationDto } from './tauriApi';

function operation(auxiliaryError: DocumentOperationDto['auxiliaryError']): DocumentOperationDto {
  return {
    document: {
      path: 'C:\\docs\\saved.md',
      fileIdentity: 'test-file:saved.md',
      contentVersion: 'sha256:saved',
      title: 'saved.md',
      content: '# Saved',
      isDirty: false,
      lastSavedAt: '2026-08-09T00:00:00Z',
      fileSize: 7
    },
    auxiliaryError
  };
}

describe('document operation results', () => {
  test('applies the primary document even when the auxiliary effect fails', () => {
    const apply = vi.fn();
    const auxiliaryError = {
      code: 'RECENT_FILES_WRITE_FAILED',
      message: '保存最近文件失败：disk full'
    };

    expect(applyDocumentOperation(operation(auxiliaryError), apply)).toEqual(auxiliaryError);
    expect(apply).toHaveBeenCalledWith(expect.objectContaining({ title: 'saved.md' }));
  });

  test('returns null after applying a fully successful result', () => {
    const apply = vi.fn();

    expect(applyDocumentOperation(operation(null), apply)).toBeNull();
    expect(apply).toHaveBeenCalledOnce();
  });
});
