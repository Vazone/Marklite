import type { AppError, DocumentDto, DocumentOperationDto } from './tauriApi';

export function applyDocumentOperation(
  operation: DocumentOperationDto,
  applyDocument: (document: DocumentDto) => void
): AppError | null {
  applyDocument(operation.document);
  return operation.auxiliaryError;
}
