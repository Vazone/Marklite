import type { DocumentDto, DocumentOperationDto } from './tauriApi';

export type OpenedDocument<TTab> = {
  tab: TTab;
  result: DocumentOperationDto;
};

export async function openDocumentPath<TTab>(
  path: string,
  openFile: (path: string) => Promise<DocumentOperationDto>,
  openDocument: (document: DocumentDto) => TTab,
  refreshRecentFiles: () => Promise<unknown>
): Promise<OpenedDocument<TTab>> {
  const result = await openFile(path);
  const tab = openDocument(result.document);
  void refreshRecentFiles();
  return { tab, result };
}
