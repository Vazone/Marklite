import { describe, expect, test } from 'vitest';
import {
  buildCommandItems,
  codeMirrorShortcutFor,
  commandDefinitions,
  findKeyboardCommand,
  shortcutFor
} from './commands';
import { listedShortcuts } from './shortcuts';

function keyboard(key: string, overrides: KeyboardEventInit = {}): KeyboardEvent {
  return new KeyboardEvent('keydown', { key, ctrlKey: true, ...overrides });
}

describe('command registry', () => {
  test('keeps ids and keyboard chords unique', () => {
    expect(new Set(commandDefinitions.map((item) => item.id)).size).toBe(commandDefinitions.length);
    const chords = commandDefinitions
      .filter((item) => item.shortcut)
      .map((item) => `${item.shortcut!.key}:${Boolean(item.shortcut!.shift)}`);
    expect(new Set(chords).size).toBe(chords.length);
  });

  test('resolves global keyboard commands from the same metadata shown in the UI', () => {
    expect(findKeyboardCommand(keyboard('s'))).toBe('save');
    expect(findKeyboardCommand(keyboard('S', { shiftKey: true }))).toBe('save-as');
    expect(findKeyboardCommand(keyboard('e'))).toBe('toggle-preview');
    expect(shortcutFor('save', 'Win32')).toBe('Ctrl+S');
    expect(shortcutFor('save', 'MacIntel')).toBe('Cmd+S');
    expect(codeMirrorShortcutFor('format-bold')).toBe('Mod-b');
  });

  test('derives palette and settings shortcut labels without duplicate facts', () => {
    const actions = Object.fromEntries(
      commandDefinitions.map((definition) => [definition.id, () => undefined])
    );
    const palette = buildCommandItems(actions, 'MacIntel');
    expect(palette.find((item) => item.id === 'toggle-preview')).toMatchObject({
      title: 'Toggle preview',
      shortcut: 'Cmd+E'
    });
    expect(listedShortcuts('MacIntel')).toContainEqual({ keys: 'Cmd+B', action: 'Bold' });
  });
});
