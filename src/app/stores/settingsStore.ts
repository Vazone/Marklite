import { writable } from 'svelte/store';
import { api, defaultSettings, toAppError, type AppSettings } from '../../lib/tauriApi';
import { localizeError, setLanguage, t } from '../../lib/i18n';
import { uiActions } from './uiStore';

const operationBusyStore = writable(false);
export const settingsOperationBusy = { subscribe: operationBusyStore.subscribe };

function createSettingsStore() {
  const store = writable<AppSettings>({ ...defaultSettings });
  let localRevision = 0;
  let intentRevision = 0;
  let pendingOperations = 0;
  let operationQueue: Promise<void> = Promise.resolve();

  function enqueue<T>(operation: () => Promise<T>): Promise<T> {
    pendingOperations += 1;
    operationBusyStore.set(true);
    const result = operationQueue.then(operation, operation);
    operationQueue = result.then(() => undefined, () => undefined);
    return result.finally(() => {
      pendingOperations -= 1;
      operationBusyStore.set(pendingOperations > 0);
    });
  }

  function commitSavedSettings(
    settings: AppSettings,
    requestIntentRevision: number,
    requestLocalRevision: number
  ) {
    if (intentRevision !== requestIntentRevision || localRevision !== requestLocalRevision) return;
    setLanguage(settings.language);
    store.set(settings);
  }

  function persist(settings: AppSettings) {
    const requestIntentRevision = ++intentRevision;
    const requestRevision = localRevision;
    return enqueue(async () => {
      try {
        const saved = await api.updateSettings(settings);
        commitSavedSettings(saved, requestIntentRevision, requestRevision);
        if (intentRevision === requestIntentRevision) {
          uiActions.toast(t('toast.settingsSaved'));
        }
        return saved;
      } catch (error) {
        uiActions.toast(localizeError(toAppError(error)), 'error');
        throw error;
      }
    });
  }

  return {
    subscribe: store.subscribe,
    setLocal(settings: AppSettings) {
      localRevision += 1;
      setLanguage(settings.language);
      store.set(settings);
    },
    async load() {
      try {
        const settings = await api.getSettings();
        setLanguage(settings.language);
        store.set(settings);
        return true;
      } catch (error) {
        uiActions.toast(localizeError(toAppError(error)), 'error');
        setLanguage(defaultSettings.language);
        store.set({ ...defaultSettings });
        return false;
      }
    },
    async save(settings: AppSettings) {
      return persist(settings);
    },
    reset() {
      const requestIntentRevision = ++intentRevision;
      const requestRevision = localRevision;
      return enqueue(async () => {
        try {
          const settings = await api.resetSettings();
          commitSavedSettings(settings, requestIntentRevision, requestRevision);
          if (intentRevision === requestIntentRevision) {
            uiActions.toast(t('toast.settingsReset'));
          }
          return settings;
        } catch (error) {
          uiActions.toast(localizeError(toAppError(error)), 'error');
          throw error;
        }
      });
    }
  };
}

export const settingsStore = createSettingsStore();
