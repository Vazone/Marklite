import { getLanguage, translate, type AppLanguage } from './i18n';
import type { MessageKey } from './i18n/messages';

export type CommandId =
  | 'new'
  | 'open'
  | 'save'
  | 'save-as'
  | 'close-tab'
  | 'export'
  | 'find'
  | 'replace'
  | 'toggle-preview'
  | 'layout-edit'
  | 'layout-split'
  | 'layout-preview'
  | 'sidebar'
  | 'settings'
  | 'command-palette'
  | 'export-startup-diagnostics'
  | 'clear-startup-diagnostics'
  | 'markdown-guide'
  | 'about'
  | 'format-bold'
  | 'format-italic'
  | 'insert-link';

export type CommandShortcut = {
  key: string;
  modifier: 'primary';
  shift?: boolean;
};

export type CommandDefinition = {
  id: CommandId;
  titleKey: MessageKey;
  categoryKey: MessageKey;
  shortcut?: CommandShortcut;
  palette: boolean;
};

export type CommandItem = {
  id: CommandId;
  title: string;
  category: string;
  shortcut?: string;
  action: () => void;
};

const primary = (key: string, shift = false): CommandShortcut => ({
  key,
  modifier: 'primary',
  ...(shift ? { shift: true } : {})
});

export const commandDefinitions: readonly CommandDefinition[] = [
  { id: 'new', titleKey: 'command.new', categoryKey: 'command.category.file', shortcut: primary('n'), palette: true },
  { id: 'open', titleKey: 'command.open', categoryKey: 'command.category.file', shortcut: primary('o'), palette: true },
  { id: 'save', titleKey: 'command.save', categoryKey: 'command.category.file', shortcut: primary('s'), palette: true },
  { id: 'save-as', titleKey: 'command.saveAs', categoryKey: 'command.category.file', shortcut: primary('s', true), palette: true },
  { id: 'close-tab', titleKey: 'command.closeTab', categoryKey: 'command.category.file', shortcut: primary('w'), palette: true },
  { id: 'export', titleKey: 'command.export', categoryKey: 'command.category.file', palette: true },
  { id: 'find', titleKey: 'command.find', categoryKey: 'command.category.edit', shortcut: primary('f'), palette: true },
  { id: 'replace', titleKey: 'command.replace', categoryKey: 'command.category.edit', shortcut: primary('h'), palette: true },
  { id: 'toggle-preview', titleKey: 'command.togglePreview', categoryKey: 'command.category.view', shortcut: primary('e'), palette: true },
  { id: 'layout-edit', titleKey: 'command.layoutEdit', categoryKey: 'command.category.view', palette: true },
  { id: 'layout-split', titleKey: 'command.layoutSplit', categoryKey: 'command.category.view', palette: true },
  { id: 'layout-preview', titleKey: 'command.layoutPreview', categoryKey: 'command.category.view', palette: true },
  { id: 'sidebar', titleKey: 'command.sidebar', categoryKey: 'command.category.view', palette: true },
  { id: 'settings', titleKey: 'command.settings', categoryKey: 'command.category.tools', shortcut: primary(','), palette: true },
  { id: 'command-palette', titleKey: 'command.palette', categoryKey: 'command.category.tools', shortcut: primary('p'), palette: false },
  { id: 'export-startup-diagnostics', titleKey: 'command.exportDiagnostics', categoryKey: 'command.category.help', palette: true },
  { id: 'clear-startup-diagnostics', titleKey: 'command.clearDiagnostics', categoryKey: 'command.category.help', palette: true },
  { id: 'markdown-guide', titleKey: 'command.markdownGuide', categoryKey: 'command.category.help', palette: true },
  { id: 'about', titleKey: 'command.about', categoryKey: 'command.category.help', palette: true },
  { id: 'format-bold', titleKey: 'command.bold', categoryKey: 'command.category.markdown', shortcut: primary('b'), palette: false },
  { id: 'format-italic', titleKey: 'command.italic', categoryKey: 'command.category.markdown', shortcut: primary('i'), palette: false },
  { id: 'insert-link', titleKey: 'command.link', categoryKey: 'command.category.markdown', shortcut: primary('k'), palette: false }
];

export type CommandActions = Partial<Record<CommandId, () => void>>;

export function buildCommandItems(
  actions: CommandActions,
  platform: string = navigator.platform,
  language: AppLanguage = getLanguage()
): CommandItem[] {
  return commandDefinitions.flatMap((definition) => {
    const action = actions[definition.id];
    if (!definition.palette || !action) return [];
    return [{
      id: definition.id,
      title: translate(language, definition.titleKey),
      category: translate(language, definition.categoryKey),
      shortcut: definition.shortcut ? shortcutLabel(definition.shortcut, platform) : undefined,
      action
    }];
  });
}

export function findKeyboardCommand(event: KeyboardEvent): CommandId | null {
  const definition = commandDefinitions.find(
    (candidate) => candidate.shortcut && matchesShortcut(event, candidate.shortcut)
  );
  return definition?.id ?? null;
}

export function matchesShortcut(event: KeyboardEvent, shortcut: CommandShortcut): boolean {
  const primaryPressed = event.ctrlKey || event.metaKey;
  return (
    primaryPressed &&
    !event.altKey &&
    event.shiftKey === Boolean(shortcut.shift) &&
    event.key.toLocaleLowerCase() === shortcut.key.toLocaleLowerCase()
  );
}

export function shortcutLabel(shortcut: CommandShortcut, platform: string = navigator.platform): string {
  const modifier = /mac|iphone|ipad|ipod/i.test(platform) ? 'Cmd' : 'Ctrl';
  const key = shortcut.key.length === 1 ? shortcut.key.toLocaleUpperCase() : shortcut.key;
  return [modifier, ...(shortcut.shift ? ['Shift'] : []), key].join('+');
}

export function shortcutFor(id: CommandId, platform: string = navigator.platform): string | undefined {
  const shortcut = commandDefinitions.find((definition) => definition.id === id)?.shortcut;
  return shortcut ? shortcutLabel(shortcut, platform) : undefined;
}

export function codeMirrorShortcutFor(id: CommandId): string {
  const shortcut = commandDefinitions.find((definition) => definition.id === id)?.shortcut;
  if (!shortcut) throw new Error(translate(getLanguage(), 'command.noEditorShortcut', { id }));
  const key = shortcut.key.length === 1 ? shortcut.key.toLocaleLowerCase() : shortcut.key;
  return ['Mod', ...(shortcut.shift ? ['Shift'] : []), key].join('-');
}
