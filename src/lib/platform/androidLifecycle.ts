import { invoke } from '@tauri-apps/api/core';

export async function exitAndroidApplication(): Promise<void> {
  await invoke('exit_android_application');
}

export async function setAndroidStatusBarAppearance(dark: boolean): Promise<void> {
  await invoke('set_android_status_bar_appearance', { dark });
}

export type AndroidSystemInsets = {
  top: number; right: number; bottom: number; left: number; imeBottom: number;
};

export function androidAvailableViewportHeight(
  layoutHeight: number,
  visualHeight: number,
  imeBottom: number
): number {
  // WebView before M139 does not shrink visualViewport for the IME. Newer
  // versions do, so take the smaller measured space rather than subtracting twice.
  return Math.max(0, Math.min(visualHeight, layoutHeight - imeBottom));
}

export function parseAndroidSystemInsets(value: unknown): AndroidSystemInsets {
  if (!value || typeof value !== 'object' || Array.isArray(value) ||
    !(['top', 'right', 'bottom', 'left', 'imeBottom'] as const).every(key => {
      const inset = (value as Record<string, unknown>)[key];
      return typeof inset === 'number' && Number.isFinite(inset) && inset >= 0 &&
        inset <= (key === 'imeBottom' ? 8192 : 256);
    })) {
    throw { code: 'INVALID_RESPONSE', message: 'Invalid Android system insets.' };
  }
  return value as AndroidSystemInsets;
}

export async function getAndroidSystemInsets(): Promise<AndroidSystemInsets> {
  const value: unknown = await invoke('android_system_insets');
  return parseAndroidSystemInsets(value);
}
