import { describe, expect, it } from 'vitest';
import { findKeyboardCommand } from './commands';
import { shouldHandleGlobalShortcut } from './shortcutGuard';

const noModal = {
  settingsOpen: false,
  commandPaletteOpen: false,
  aboutOpen: false,
  markdownGuideOpen: false,
  exitConfirmationOpen: false,
  exportDialogOpen: false
};

describe('global shortcut guard', () => {
  it('does not let global document commands escape a modal', () => {
    const event = new KeyboardEvent('keydown', { key: 'n', ctrlKey: true });
    expect(shouldHandleGlobalShortcut(event, { ...noModal, settingsOpen: true })).toBe(false);
  });

  it('does not take shortcuts from editable controls or already handled events', () => {
    const input = document.createElement('input');
    document.body.append(input);
    const event = new KeyboardEvent('keydown', { key: 'f', ctrlKey: true, bubbles: true, cancelable: true });
    input.dispatchEvent(event);
    expect(shouldHandleGlobalShortcut(event, noModal)).toBe(false);

    const handled = new KeyboardEvent('keydown', { key: 's', ctrlKey: true, cancelable: true });
    handled.preventDefault();
    expect(shouldHandleGlobalShortcut(handled, noModal)).toBe(false);
  });

  it('lets unhandled application commands leave the MarkLite editor', () => {
    const editor = document.createElement('div');
    editor.dataset.markliteApplicationShortcuts = 'true';
    const content = document.createElement('div');
    content.setAttribute('contenteditable', 'true');
    editor.append(content);

    const save = new KeyboardEvent('keydown', { key: 's', ctrlKey: true, bubbles: true, cancelable: true });
    Object.defineProperty(save, 'target', { value: content });

    expect(shouldHandleGlobalShortcut(save, noModal)).toBe(true);
    expect(findKeyboardCommand(save)).toBe('save');
  });

  it('keeps unrelated editable regions and editor-owned commands isolated', () => {
    const unrelated = document.createElement('div');
    unrelated.setAttribute('contenteditable', 'true');
    const save = new KeyboardEvent('keydown', { key: 's', ctrlKey: true });
    Object.defineProperty(save, 'target', { value: unrelated });
    expect(shouldHandleGlobalShortcut(save, noModal)).toBe(false);

    const editor = document.createElement('div');
    editor.dataset.markliteApplicationShortcuts = 'true';
    const content = document.createElement('div');
    content.setAttribute('contenteditable', 'true');
    editor.append(content);
    const handled = new KeyboardEvent('keydown', { key: 'b', ctrlKey: true, cancelable: true });
    Object.defineProperty(handled, 'target', { value: content });
    handled.preventDefault();
    expect(shouldHandleGlobalShortcut(handled, noModal)).toBe(false);
  });

  it('allows a global shortcut from the application shell', () => {
    const shell = document.createElement('div');
    const event = new KeyboardEvent('keydown', { key: 's', ctrlKey: true });
    Object.defineProperty(event, 'target', { value: shell });
    expect(shouldHandleGlobalShortcut(event, noModal)).toBe(true);
  });
});
