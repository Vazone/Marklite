export type InstallBarrierResult = 'ready' | 'busy' | 'cancelled' | 'save-failed';

type InstallBarrierDependencies<Document> = {
  flushEditor: () => void;
  isBusy: () => boolean;
  dirtyDocuments: () => Document[];
  confirmSave: (count: number) => Promise<boolean>;
  saveDocument: (document: Document) => Promise<boolean>;
  flushPersistence: () => Promise<void>;
};

export async function prepareInstall<Document>(deps: InstallBarrierDependencies<Document>): Promise<InstallBarrierResult> {
  deps.flushEditor();
  if (deps.isBusy()) return 'busy';

  const dirty = deps.dirtyDocuments();
  if (dirty.length && !await deps.confirmSave(dirty.length)) return 'cancelled';
  if (deps.isBusy()) return 'busy';

  for (const document of dirty) {
    if (!await deps.saveDocument(document)) return 'save-failed';
    if (deps.isBusy()) return 'busy';
  }

  await deps.flushPersistence();
  return deps.isBusy() || deps.dirtyDocuments().length ? 'save-failed' : 'ready';
}
