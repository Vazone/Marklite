import { derived, get, writable } from 'svelte/store';
import type { AppError } from '../platform/contracts';
import { enMessages, type MessageKey, zhCnMessages } from './messages';

export const languageRegistry = {
  en: { label: 'English', messages: enMessages },
  'zh-CN': { label: '简体中文', messages: zhCnMessages }
} as const;

export type AppLanguage = keyof typeof languageRegistry;
export type TranslationValues = Readonly<Record<string, string | number>>;
export type Translator = (key: MessageKey, values?: TranslationValues) => string;

export const supportedLanguages = (Object.keys(languageRegistry) as AppLanguage[]).map((id) => ({
  id,
  label: languageRegistry[id].label
}));

const languageStore = writable<AppLanguage>('en');

export const currentLanguage = { subscribe: languageStore.subscribe };
export const translator = derived(languageStore, (language): Translator =>
  (key, values) => translate(language, key, values)
);

export function setLanguage(language: AppLanguage): void {
  const resolved = isAppLanguage(language) ? language : 'en';
  languageStore.set(resolved);
  if (typeof document !== 'undefined') document.documentElement.lang = resolved;
}

export function getLanguage(): AppLanguage {
  return get(languageStore);
}

export function isAppLanguage(value: unknown): value is AppLanguage {
  return typeof value === 'string' && Object.hasOwn(languageRegistry, value);
}

export function translate(
  language: AppLanguage,
  key: MessageKey,
  values: TranslationValues = {}
): string {
  const catalog = languageRegistry[language]?.messages ?? enMessages;
  const template = catalog[key] ?? enMessages[key] ?? key;
  return template.replace(/\{([A-Za-z][A-Za-z0-9]*)\}/g, (match, name: string) =>
    Object.hasOwn(values, name) ? String(values[name]) : match
  );
}

export function t(key: MessageKey, values?: TranslationValues): string {
  return translate(getLanguage(), key, values);
}

const errorKeys = new Set<MessageKey>(
  Object.keys(enMessages).filter((key): key is MessageKey => key.startsWith('error.'))
);

export function localizeError(error: AppError): string {
  const key = `error.${error.code}` as MessageKey;
  return errorKeys.has(key)
    ? t(key)
    : t('error.generic', { code: error.code || 'UNKNOWN_ERROR' });
}
