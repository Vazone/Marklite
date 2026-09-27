import { expect, test, vi } from 'vitest';
import { createDesktopLinks, createResourceLinks } from './resourceLinks';
import { desktopFile } from './resources';
import type { TauriClient } from './tauriClient';

test('relative targets stay opaque and source URI fails before invoking desktop access', async () => {
  const client = { resolveMarkdownTarget: vi.fn(), loadLocalImages: vi.fn(), cancelLocalImageJob: vi.fn() };
  const links = createDesktopLinks(client);
  const source = desktopFile('C:\\notes\\a%23.md');
  await links.resolve(source, '../b%20c.md#heading');
  expect(client.resolveMarkdownTarget).toHaveBeenCalledWith('C:\\notes\\a%23.md', '../b%20c.md#heading');
  await links.images(source, ['./image%23.png'], 'job');
  expect(client.loadLocalImages).toHaveBeenCalledWith('C:\\notes\\a%23.md', ['./image%23.png'], 'job');
  await links.resolve(null, '#heading');
  expect(client.resolveMarkdownTarget).toHaveBeenLastCalledWith(null, '#heading');
  const uri = { kind: 'androidDocument' as const, uri: 'content://provider/document/a' };
  await expect(links.resolve(uri, '../b.md')).rejects.toMatchObject({ code: 'RESOURCE_UNSUPPORTED' });
  await expect(links.images(uri, ['./image.png'], 'uri-job')).rejects.toMatchObject({ code: 'RESOURCE_UNSUPPORTED' });
  expect(client.loadLocalImages).toHaveBeenCalledTimes(1);
  await links.cancelImages('job');
  expect(client.cancelLocalImageJob).toHaveBeenCalledWith('job');
});

test('Android relative links and images use the typed native resource boundary', async () => {
  const source = { kind: 'androidDocument' as const, uri: 'content://provider/tree/root/document/a' };
  const client = {
    getPlatformCapabilities: vi.fn(async () => ({ platform: 'android', desktopFiles: false, documentUris: true, exportFormats: [] })),
    resolveMarkdownTarget: vi.fn(), loadLocalImages: vi.fn(), cancelLocalImageJob: vi.fn()
  } as unknown as Pick<TauriClient, 'getPlatformCapabilities' | 'resolveMarkdownTarget' | 'loadLocalImages' | 'cancelLocalImageJob'>;
  const metadata = new TextEncoder().encode(JSON.stringify({ entries: [
    { target: '../x.png', resourceIndex: null, error: { code: 'IMAGE_MISSING', message: 'Missing' } }
  ], resources: [] }));
  const body = new Uint8Array(4 + metadata.length);
  new DataView(body.buffer).setUint32(0, metadata.length, true);
  body.set(metadata, 4);
  const call = vi.fn(async (command: string) => command === 'load_android_images' ? body :
    { kind: 'localDocument', path: null, resource: source, fragment: 'section' });
  const links = createResourceLinks(client, call as unknown as typeof import('@tauri-apps/api/core').invoke);
  expect(await links.resolve(source, '../b.md#section')).toEqual({ kind: 'localDocument', path: null, resource: source, fragment: 'section' });
  expect(call).toHaveBeenCalledWith('resolve_android_markdown_target', { source, target: '../b.md#section' });
  const batch = await links.images(source, ['../x.png'], 'job');
  expect(batch.entries[0].error?.code).toBe('IMAGE_MISSING');
  expect(call).toHaveBeenCalledWith('load_android_images', { source, targets: ['../x.png'], jobId: 'job' });
  expect(client.resolveMarkdownTarget).not.toHaveBeenCalled();
  expect(client.loadLocalImages).not.toHaveBeenCalled();
});
