export type DirtyExitDocument = {
  id: string;
  title: string;
  path: string | null;
  contentRevision: number;
};

export type ExitPromptState = {
  documents: DirtyExitDocument[];
  busy: boolean;
  busyLabel: string;
};

export type ExitProtectionCallbacks = {
  getDirtyDocuments: () => DirtyExitDocument[];
  onPromptChange: (state: ExitPromptState | null) => void;
  saveDocument: (document: DirtyExitDocument) => Promise<boolean>;
  closeWindow: () => Promise<void>;
  onSaveIncomplete?: (document: DirtyExitDocument, error?: unknown) => void;
  onCloseError?: (error: unknown) => void;
};

type ExitPhase = 'idle' | 'prompt' | 'saving' | 'closing';

function snapshotDocuments(documents: DirtyExitDocument[]): DirtyExitDocument[] {
  return documents.map((document) => ({ ...document }));
}

export function createExitProtectionController(callbacks: ExitProtectionCallbacks) {
  let phase: ExitPhase = 'idle';
  let promptDocuments: DirtyExitDocument[] = [];

  function publishPrompt(busy: boolean, busyLabel = ''): void {
    callbacks.onPromptChange({
      documents: snapshotDocuments(promptDocuments),
      busy,
      busyLabel
    });
  }

  function restorePrompt(): void {
    const currentDirty = callbacks.getDirtyDocuments();
    if (currentDirty.length) {
      promptDocuments = snapshotDocuments(currentDirty);
      phase = 'prompt';
      publishPrompt(false);
      return;
    }

    phase = 'idle';
    promptDocuments = [];
    callbacks.onPromptChange(null);
  }

  async function closeAfterDecision(busyLabel: string): Promise<boolean> {
    phase = 'closing';
    publishPrompt(true, busyLabel);
    try {
      await callbacks.closeWindow();
      phase = 'idle';
      promptDocuments = [];
      callbacks.onPromptChange(null);
      return true;
    } catch (error) {
      callbacks.onCloseError?.(error);
      restorePrompt();
      return false;
    }
  }

  return {
    handleCloseRequest(preventDefault: () => void): boolean {
      if (phase !== 'idle') {
        preventDefault();
        return true;
      }

      const dirtyDocuments = callbacks.getDirtyDocuments();
      if (!dirtyDocuments.length) return false;

      preventDefault();
      promptDocuments = snapshotDocuments(dirtyDocuments);
      phase = 'prompt';
      publishPrompt(false);
      return true;
    },

    cancel(): boolean {
      if (phase !== 'prompt') return false;
      phase = 'idle';
      promptDocuments = [];
      callbacks.onPromptChange(null);
      return true;
    },

    async discardAndExit(): Promise<boolean> {
      if (phase !== 'prompt') return false;
      return closeAfterDecision('正在退出…');
    },

    async saveAndExit(): Promise<boolean> {
      if (phase !== 'prompt') return false;

      phase = 'saving';
      publishPrompt(true, '正在保存未保存的文档…');
      const targets = snapshotDocuments(callbacks.getDirtyDocuments());

      for (const target of targets) {
        const current = callbacks.getDirtyDocuments().find((document) => document.id === target.id);
        if (!current) continue;

        try {
          const saved = await callbacks.saveDocument(current);
          const stillDirty = callbacks.getDirtyDocuments().find((document) => document.id === current.id);
          if (!saved || stillDirty) {
            callbacks.onSaveIncomplete?.(stillDirty ?? current);
            restorePrompt();
            return false;
          }
        } catch (error) {
          callbacks.onSaveIncomplete?.(current, error);
          restorePrompt();
          return false;
        }
      }

      const remaining = callbacks.getDirtyDocuments();
      if (remaining.length) {
        callbacks.onSaveIncomplete?.(remaining[0]);
        restorePrompt();
        return false;
      }

      return closeAfterDecision('正在退出…');
    }
  };
}
