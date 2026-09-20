/// <reference types="node" />

import { readFileSync } from 'node:fs';
import { describe, expect, test, vi } from 'vitest';
import {
  createExitProtectionController,
  type DirtyExitDocument,
  type ExitPromptState
} from './exitProtection';

function document(id: string, revision = 1, path: string | null = `C:\\docs\\${id}.md`): DirtyExitDocument {
  return {
    id,
    title: `${id}.md`,
    path,
    contentRevision: revision
  };
}

function setup(initialDirty: DirtyExitDocument[] = []) {
  let dirty = initialDirty.map((item) => ({ ...item }));
  const promptStates: Array<ExitPromptState | null> = [];
  const closeWindow = vi.fn(async () => undefined);
  const saveDocument = vi.fn(async (item: DirtyExitDocument) => {
    dirty = dirty.filter((candidate) => candidate.id !== item.id);
    return true;
  });
  const onSaveIncomplete = vi.fn();
  const onCloseError = vi.fn();
  const controller = createExitProtectionController({
    getDirtyDocuments: () => dirty.map((item) => ({ ...item })),
    onPromptChange: (state) => promptStates.push(state),
    saveDocument,
    closeWindow,
    onSaveIncomplete,
    onCloseError
  });

  return {
    controller,
    closeWindow,
    saveDocument,
    onSaveIncomplete,
    onCloseError,
    promptStates,
    setDirty(next: DirtyExitDocument[]) {
      dirty = next.map((item) => ({ ...item }));
    },
    getDirty() {
      return dirty;
    }
  };
}

describe('exit protection close requests', () => {
  test('intercepts a clean close and waits for the shared close callback', async () => {
    const { controller, promptStates, closeWindow } = setup();
    const preventDefault = vi.fn();

    expect(controller.handleCloseRequest(preventDefault)).toBe(true);
    expect(preventDefault).toHaveBeenCalledOnce();
    await vi.waitFor(() => expect(closeWindow).toHaveBeenCalledOnce());
    expect(promptStates).toEqual([null]);
  });

  test('a failed clean close is observable and a repeated close can retry', async () => {
    const state = setup();
    const failure = new Error('session flush failed');
    state.closeWindow.mockRejectedValueOnce(failure);

    expect(state.controller.handleCloseRequest(vi.fn())).toBe(true);
    await vi.waitFor(() => expect(state.onCloseError).toHaveBeenCalledWith(failure));
    expect(state.controller.handleCloseRequest(vi.fn())).toBe(true);
    await vi.waitFor(() => expect(state.closeWindow).toHaveBeenCalledTimes(2));
  });

  test('prevents dirty close requests and never stacks prompts', () => {
    const { controller, promptStates } = setup([document('first'), document('second', 2, null)]);
    const firstPrevent = vi.fn();
    const repeatedPrevent = vi.fn();

    expect(controller.handleCloseRequest(firstPrevent)).toBe(true);
    expect(controller.handleCloseRequest(repeatedPrevent)).toBe(true);

    expect(firstPrevent).toHaveBeenCalledOnce();
    expect(repeatedPrevent).toHaveBeenCalledOnce();
    expect(promptStates).toHaveLength(1);
    expect(promptStates[0]).toMatchObject({
      busy: false,
      documents: [{ id: 'first' }, { id: 'second' }]
    });
  });

  test('cancel dismisses the prompt without changing dirty documents', () => {
    const dirty = [document('draft', 3, null)];
    const { controller, getDirty, promptStates, closeWindow } = setup(dirty);

    controller.handleCloseRequest(vi.fn());
    expect(controller.cancel()).toBe(true);

    expect(getDirty()).toEqual(dirty);
    expect(promptStates.at(-1)).toBeNull();
    expect(closeWindow).not.toHaveBeenCalled();
  });
});

describe('exit protection decisions', () => {
  test('discard exits without saving', async () => {
    const { controller, closeWindow, saveDocument } = setup([document('draft')]);
    controller.handleCloseRequest(vi.fn());

    await expect(controller.discardAndExit()).resolves.toBe(true);

    expect(saveDocument).not.toHaveBeenCalled();
    expect(closeWindow).toHaveBeenCalledOnce();
  });

  test('save and exit processes every dirty document in tab order', async () => {
    const { controller, closeWindow, saveDocument, promptStates } = setup([
      document('first'),
      document('untitled', 4, null),
      document('third')
    ]);
    controller.handleCloseRequest(vi.fn());

    await expect(controller.saveAndExit()).resolves.toBe(true);

    expect(saveDocument.mock.calls.map(([item]) => item.id)).toEqual(['first', 'untitled', 'third']);
    expect(closeWindow).toHaveBeenCalledOnce();
    expect(promptStates.some((state) => state?.busyLabel.includes('Saving'))).toBe(true);
    expect(promptStates.at(-1)).toBeNull();
  });

  test('a canceled or failed save keeps the window open and restores the prompt', async () => {
    const state = setup([document('first'), document('second')]);
    state.saveDocument.mockImplementationOnce(async () => false);
    state.controller.handleCloseRequest(vi.fn());

    await expect(state.controller.saveAndExit()).resolves.toBe(false);

    expect(state.closeWindow).not.toHaveBeenCalled();
    expect(state.onSaveIncomplete).toHaveBeenCalledWith(expect.objectContaining({ id: 'first' }));
    expect(state.promptStates.at(-1)).toMatchObject({ busy: false, documents: [{ id: 'first' }, { id: 'second' }] });
  });

  test('a new revision that remains dirty after a save prevents exit', async () => {
    const state = setup([document('draft', 1)]);
    state.saveDocument.mockImplementationOnce(async () => {
      state.setDirty([document('draft', 2)]);
      return true;
    });
    state.controller.handleCloseRequest(vi.fn());

    await expect(state.controller.saveAndExit()).resolves.toBe(false);

    expect(state.closeWindow).not.toHaveBeenCalled();
    expect(state.promptStates.at(-1)).toMatchObject({
      busy: false,
      documents: [{ id: 'draft', contentRevision: 2 }]
    });
  });

  test('does not start another decision while a save is in progress', async () => {
    const state = setup([document('draft')]);
    let finishSave: (() => void) | undefined;
    state.saveDocument.mockImplementationOnce(
      () =>
        new Promise<boolean>((resolve) => {
          finishSave = () => {
            state.setDirty([]);
            resolve(true);
          };
        })
    );
    state.controller.handleCloseRequest(vi.fn());

    const firstDecision = state.controller.saveAndExit();
    await vi.waitFor(() => expect(state.saveDocument).toHaveBeenCalledOnce());
    await expect(state.controller.saveAndExit()).resolves.toBe(false);
    await expect(state.controller.discardAndExit()).resolves.toBe(false);
    expect(state.controller.cancel()).toBe(false);
    const repeatedPrevent = vi.fn();
    expect(state.controller.handleCloseRequest(repeatedPrevent)).toBe(true);
    expect(repeatedPrevent).toHaveBeenCalledOnce();

    finishSave?.();
    await expect(firstDecision).resolves.toBe(true);
    expect(state.closeWindow).toHaveBeenCalledOnce();
  });

  test('a native close error is reported and does not leave the controller busy', async () => {
    const state = setup([document('draft')]);
    const failure = new Error('window close failed');
    state.closeWindow.mockRejectedValueOnce(failure);
    state.controller.handleCloseRequest(vi.fn());

    await expect(state.controller.discardAndExit()).resolves.toBe(false);

    expect(state.onCloseError).toHaveBeenCalledWith(failure);
    expect(state.promptStates.at(-1)).toMatchObject({ busy: false, documents: [{ id: 'draft' }] });
    state.closeWindow.mockResolvedValueOnce(undefined);
    await expect(state.controller.discardAndExit()).resolves.toBe(true);
    expect(state.closeWindow).toHaveBeenCalledTimes(2);
    expect(state.promptStates.at(-1)).toBeNull();
  });

  test('returns to idle after the host close callback so a surviving window can request exit again', async () => {
    const state = setup([document('draft')]);
    state.controller.handleCloseRequest(vi.fn());

    await expect(state.controller.discardAndExit()).resolves.toBe(true);
    expect(state.promptStates.at(-1)).toBeNull();

    const repeatedPrevent = vi.fn();
    expect(state.controller.handleCloseRequest(repeatedPrevent)).toBe(true);
    expect(repeatedPrevent).toHaveBeenCalledOnce();
    expect(state.promptStates.at(-1)).toMatchObject({ busy: false, documents: [{ id: 'draft' }] });
  });
});

describe('Tauri exit capability contract', () => {
  test('grants only the required destructive main-window command', () => {
    const capability = JSON.parse(readFileSync('src-tauri/capabilities/default.json', 'utf8')) as {
      windows: string[];
      permissions: Array<
        string | { identifier: string; allow: Array<{ url: string }> }
      >;
    };

    expect(capability.windows).toEqual(['main']);
    expect(capability.permissions).toEqual([
      'core:default',
      'core:window:allow-destroy',
      'dialog:default',
      {
        identifier: 'opener:allow-open-url',
        allow: [{ url: 'http://*' }, { url: 'https://*' }]
      }
    ]);
  });
});
