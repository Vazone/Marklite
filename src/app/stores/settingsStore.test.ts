import { get } from 'svelte/store';
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';
import { api, defaultSettings } from '../../lib/tauriApi';
import { settingsOperationBusy, settingsStore } from './settingsStore';
import { uiActions, uiStore } from './uiStore';

function clearToasts() {
  for (const toast of get(uiStore).toasts) {
    uiActions.dismissToast(toast.id);
  }
}

describe('settings store error handling', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    clearToasts();
  });

  afterEach(() => {
    clearToasts();
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  test('falls back to defaults and exposes a localized toast when loading fails', async () => {
    vi.spyOn(api, 'getSettings').mockRejectedValue({
      code: 'SETTINGS_READ_FAILED',
      message: '设置文件无法读取'
    });

    await expect(settingsStore.load()).resolves.toBe(false);

    expect(get(settingsStore)).toEqual(defaultSettings);
    expect(get(uiStore).toasts.at(-1)).toMatchObject({
      message: 'Settings could not be read. Defaults were restored when safe.',
      tone: 'error'
    });
  });

  test('keeps the persisted state and rejects when saving fails', async () => {
    const previous = { ...defaultSettings };
    vi.spyOn(api, 'getSettings').mockResolvedValue(previous);
    vi.spyOn(api, 'updateSettings').mockRejectedValue({
      code: 'INVALID_SETTINGS',
      message: '设置值无效：recentFilesLimit 必须在 3–50 之间'
    });
    await expect(settingsStore.load()).resolves.toBe(true);

    await expect(
      settingsStore.save({ ...previous, recentFilesLimit: 0 })
    ).rejects.toMatchObject({ code: 'INVALID_SETTINGS' });

    expect(get(settingsStore)).toEqual(previous);
    expect(get(uiStore).toasts.at(-1)).toMatchObject({
      message: 'One or more setting values are invalid.',
      tone: 'error'
    });
  });

  test('stores and returns the backend-normalized settings on success', async () => {
    const saved = { ...defaultSettings, cornerRadius: 12 };
    vi.spyOn(api, 'updateSettings').mockResolvedValue(saved);

    await expect(settingsStore.save(saved)).resolves.toEqual(saved);

    expect(get(settingsStore)).toEqual(saved);
  });

  test('does not let a completed save overwrite newer local settings', async () => {
    let resolveSave!: (settings: typeof defaultSettings) => void;
    vi.spyOn(api, 'updateSettings').mockReturnValue(
      new Promise((resolve) => {
        resolveSave = resolve;
      })
    );
    const first = { ...defaultSettings, cornerRadius: 5 };
    settingsStore.setLocal(first);
    const pending = settingsStore.save(first);
    settingsStore.setLocal({ ...defaultSettings, cornerRadius: 10 });

    resolveSave(first);
    await pending;

    expect(get(settingsStore).cornerRadius).toBe(10);
    expect(get(uiStore).toasts).toHaveLength(1);
  });

  test('does not expose an unused partial-update API', () => {
    expect(settingsStore).not.toHaveProperty('patch');
  });

  test('serializes save intentions so the simulated disk and store end at the latest save', async () => {
    let releaseFirst!: () => void;
    const firstGate = new Promise<void>((resolve) => {
      releaseFirst = resolve;
    });
    const diskWrites: number[] = [];
    let calls = 0;
    vi.spyOn(api, 'updateSettings').mockImplementation(async (settings) => {
      calls += 1;
      if (calls === 1) await firstGate;
      diskWrites.push(settings.cornerRadius);
      return settings;
    });
    const first = { ...defaultSettings, cornerRadius: 5 };
    const second = { ...defaultSettings, cornerRadius: 10 };

    const firstSave = settingsStore.save(first);
    const secondSave = settingsStore.save(second);
    await Promise.resolve();

    expect(calls).toBe(1);
    releaseFirst();
    await Promise.all([firstSave, secondSave]);
    expect(diskWrites).toEqual([5, 10]);
    expect(get(settingsStore).cornerRadius).toBe(10);
  });

  test('serializes save then reset so defaults are the final disk and store state', async () => {
    let releaseSave!: () => void;
    const saveGate = new Promise<void>((resolve) => {
      releaseSave = resolve;
    });
    const writes: string[] = [];
    vi.spyOn(api, 'updateSettings').mockImplementation(async (settings) => {
      await saveGate;
      writes.push(`save:${settings.cornerRadius}`);
      return settings;
    });
    const reset = vi.spyOn(api, 'resetSettings').mockImplementation(async () => {
      writes.push('reset');
      return defaultSettings;
    });

    const saving = settingsStore.save({ ...defaultSettings, cornerRadius: 9 });
    const resetting = settingsStore.reset();
    await Promise.resolve();
    expect(reset).not.toHaveBeenCalled();
    expect(get(settingsOperationBusy)).toBe(true);

    releaseSave();
    await Promise.all([saving, resetting]);
    expect(writes).toEqual(['save:9', 'reset']);
    expect(get(settingsStore)).toEqual(defaultSettings);
    expect(get(settingsOperationBusy)).toBe(false);
  });

  test('serializes reset then save so the later save is the final disk and store state', async () => {
    let releaseReset!: () => void;
    const resetGate = new Promise<void>((resolve) => {
      releaseReset = resolve;
    });
    const writes: string[] = [];
    vi.spyOn(api, 'resetSettings').mockImplementation(async () => {
      await resetGate;
      writes.push('reset');
      return defaultSettings;
    });
    const update = vi.spyOn(api, 'updateSettings').mockImplementation(async (settings) => {
      writes.push(`save:${settings.cornerRadius}`);
      return settings;
    });
    const saved = { ...defaultSettings, cornerRadius: 11, language: 'zh-CN' as const };

    const resetting = settingsStore.reset();
    const saving = settingsStore.save(saved);
    await Promise.resolve();
    expect(update).not.toHaveBeenCalled();

    releaseReset();
    await Promise.all([resetting, saving]);
    expect(writes).toEqual(['reset', 'save:11']);
    expect(get(settingsStore)).toEqual(saved);
    expect(get(settingsOperationBusy)).toBe(false);
  });

  test('continues with the next intention after a failure and keeps that rejection observable', async () => {
    const failure = { code: 'SETTINGS_WRITE_FAILED', message: 'failed' };
    vi.spyOn(api, 'updateSettings')
      .mockRejectedValueOnce(failure)
      .mockImplementationOnce(async (settings) => settings);
    const first = settingsStore.save({ ...defaultSettings, cornerRadius: 4 });
    const secondSettings = { ...defaultSettings, cornerRadius: 13 };
    const second = settingsStore.save(secondSettings);

    await expect(first).rejects.toBe(failure);
    await expect(second).resolves.toEqual(secondSettings);
    expect(get(settingsStore)).toEqual(secondSettings);
    expect(get(settingsOperationBusy)).toBe(false);
    expect(get(uiStore).toasts.some((toast) => toast.tone === 'error')).toBe(true);
  });
});
