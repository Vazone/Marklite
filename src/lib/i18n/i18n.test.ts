import { get } from 'svelte/store';
import { describe, expect, it } from 'vitest';
import {
  currentLanguage,
  languageRegistry,
  setLanguage,
  supportedLanguages,
  translate,
  translator,
  type AppLanguage
} from '.';

describe('i18n registry', () => {
  it('keeps every registered catalog aligned with the English key set', () => {
    const expected = Object.keys(languageRegistry.en.messages).sort();
    for (const language of supportedLanguages) {
      expect(Object.keys(languageRegistry[language.id].messages).sort()).toEqual(expected);
    }
  });

  it('defaults to English and updates the reactive translator', () => {
    setLanguage('en');
    expect(get(currentLanguage)).toBe('en');
    expect(get(translator)('settings.title')).toBe('Settings');

    setLanguage('zh-CN');
    expect(get(translator)('settings.title')).toBe('设置');
    setLanguage('en');
  });

  it('interpolates variables while leaving missing values visible', () => {
    expect(translate('en', 'toast.opened', { title: 'README.md' })).toBe('Opened README.md');
    expect(translate('zh-CN', 'common.line', { line: 8 })).toBe('第 8 行');
    expect(translate('en', 'common.line')).toBe('Line {line}');
  });

  it('falls back to English for an unregistered locale', () => {
    setLanguage('fr' as AppLanguage);
    expect(get(currentLanguage)).toBe('en');
    expect(translate('fr' as AppLanguage, 'settings.title')).toBe('Settings');
  });
});
