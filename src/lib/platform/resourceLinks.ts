import type { LocalImageBatchDto, MarkdownTargetDto } from './contracts';
import type { TauriClient } from './tauriClient';
import { desktopPath, type ResourceRef } from './resources';
import { invoke } from '@tauri-apps/api/core';
import { decodeLocalImageResponse, parseMarkdownTarget } from './contractValidation';

/** Relative lookup is owned by the platform, including authorization and image budgets. */
export type ResourceLinks = {
  resolve(source: ResourceRef | null, target: string): Promise<MarkdownTargetDto>;
  images(source: ResourceRef | null, targets: string[], jobId: string): Promise<LocalImageBatchDto>;
  cancelImages(jobId: string): Promise<void>;
};

export function createDesktopLinks(client: Pick<TauriClient,
  'resolveMarkdownTarget' | 'loadLocalImages' | 'cancelLocalImageJob'>): ResourceLinks {
  const path = (source: ResourceRef | null) => source === null ? null : desktopPath(source);
  return {
    async resolve(source, target) { return client.resolveMarkdownTarget(path(source), target); },
    async images(source, targets, jobId) { return client.loadLocalImages(path(source), targets, jobId); },
    cancelImages: (jobId) => client.cancelLocalImageJob(jobId)
  };
}

export function createResourceLinks(client: Pick<TauriClient,
  'getPlatformCapabilities' | 'resolveMarkdownTarget' | 'loadLocalImages' | 'cancelLocalImageJob'>,
  call: typeof invoke = invoke): ResourceLinks {
  const desktop = createDesktopLinks(client);
  return {
    async resolve(source, target) {
      if ((await client.getPlatformCapabilities()).platform === 'android') {
        return parseMarkdownTarget(await call('resolve_android_markdown_target', { source, target }));
      }
      return desktop.resolve(source, target);
    },
    async images(source, targets, jobId) {
      if ((await client.getPlatformCapabilities()).platform === 'android') {
        return decodeLocalImageResponse(await call('load_android_images', { source, targets, jobId }));
      }
      return desktop.images(source, targets, jobId);
    },
    cancelImages: jobId => desktop.cancelImages(jobId)
  };
}
