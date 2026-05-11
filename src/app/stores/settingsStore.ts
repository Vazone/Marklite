import { get, writable } from 'svelte/store';
import { api, defaultSettings, toAppError, type AppSettings } from '../../lib/tauriApi';
import { uiActions } from './uiStore';

function createSettingsStore() {
  const store = writable<AppSettings>(defaultSettings);

  return {
    subscribe: store.subscribe,
    async load() {
      try {
        const settings = await api.getSettings();
        store.set(settings);
      } catch (error) {
        uiActions.toast(toAppError(error).message, 'error');
        store.set(defaultSettings);
      }
    },
    async save(settings: AppSettings) {
      try {
        const saved = await api.updateSettings(settings);
        store.set(saved);
        uiActions.toast('设置已保存');
      } catch (error) {
        uiActions.toast(toAppError(error).message, 'error');
      }
    },
    async patch(partial: Partial<AppSettings>) {
      const next = { ...get(store), ...partial };
      await this.save(next);
    },
    async reset() {
      try {
        const settings = await api.resetSettings();
        store.set(settings);
        uiActions.toast('设置已恢复默认');
      } catch (error) {
        uiActions.toast(toAppError(error).message, 'error');
      }
    }
  };
}

export const settingsStore = createSettingsStore();
