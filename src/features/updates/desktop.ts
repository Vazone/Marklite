import { getBundleType } from '@tauri-apps/api/app';
import { check } from '@tauri-apps/plugin-updater';
import type { UpdateAdapter } from './service';

export const desktopUpdateAdapter: UpdateAdapter = {
  bundleType: getBundleType,
  check: () => check({ timeout: 10_000 })
};
