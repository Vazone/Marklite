import { commandDefinitions, shortcutLabel } from './commands';
import { getLanguage, translate, type AppLanguage } from './i18n';

export type Shortcut = {
  keys: string;
  action: string;
};

export function listedShortcuts(
  platform: string = navigator.platform,
  language: AppLanguage = getLanguage()
): Shortcut[] {
  return commandDefinitions.flatMap((definition) =>
    definition.shortcut
      ? [{ keys: shortcutLabel(definition.shortcut, platform), action: translate(language, definition.titleKey) }]
      : []
  );
}
