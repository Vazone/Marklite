import { describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';
import { createUpdateService, type UpdateCandidate } from './service';

function fixture() {
  const saved = new Map<string, string>();
  const candidate: UpdateCandidate = {
    version: '0.1.7', body: 'Update notes',
    download: vi.fn(async onProgress => {
      onProgress({ event: 'Started', data: { contentLength: 100 } });
      onProgress({ event: 'Progress', data: { chunkLength: 40 } });
      onProgress({ event: 'Progress', data: { chunkLength: 60 } });
      onProgress({ event: 'Finished' });
    }),
    install: vi.fn(async () => {}),
    close: vi.fn(async () => {})
  };
  let clock = 100_000_000;
  const check = vi.fn(async (): Promise<UpdateCandidate | null> => candidate);
  const prepareInstall = vi.fn(async () => true);
  const service = createUpdateService({
    adapter: { bundleType: async () => 'nsis', check },
    preferences: { getItem: key => saved.get(key) ?? null, setItem: (key, value) => { saved.set(key, value); } },
    autoEnabled: () => true,
    prepareInstall,
    reportError: () => 'network unavailable',
    now: () => clock
  });
  return { service, candidate, saved, check, prepareInstall, advance: (ms: number) => { clock += ms; } };
}

describe('optional desktop updates', () => {
  it('checks once per day, skips a version and lets manual checks override the skip', async () => {
    const { service, candidate, check, advance } = fixture();
    await service.check();
    expect(get(service).phase).toBe('available');
    await service.dismiss(true);
    advance(24 * 60 * 60 * 1000);
    await service.check();
    expect(get(service).phase).toBe('idle');
    expect(candidate.close).toHaveBeenCalledTimes(2);
    await service.check(true);
    expect(get(service).phase).toBe('available');
    expect(check).toHaveBeenCalledTimes(3);
    await service.dispose();
  });

  it('downloads without installing and preserves the download if installation is cancelled', async () => {
    const { service, candidate, prepareInstall } = fixture();
    prepareInstall.mockResolvedValueOnce(false);
    await service.check();
    await service.download();
    expect(get(service)).toMatchObject({ phase: 'downloaded', downloadedBytes: 100, totalBytes: 100 });
    expect(candidate.install).not.toHaveBeenCalled();
    expect(await service.install()).toBe(false);
    expect(get(service).phase).toBe('downloaded');
    expect(await service.install()).toBe(true);
    expect(candidate.install).toHaveBeenCalledTimes(1);
    expect(candidate.close).toHaveBeenCalledTimes(1);
  });

  it('does not run updater commands for a DEB install', async () => {
    const { service, check } = fixture();
    const deb = createUpdateService({
      adapter: { bundleType: async () => 'deb', check },
      preferences: { getItem: () => null, setItem: () => {} },
      autoEnabled: () => true,
      prepareInstall: async () => true,
      reportError: () => 'error'
    });
    await deb.check();
    expect(get(deb).phase).toBe('idle');
    await deb.check(true);
    expect(get(deb).phase).toBe('manual');
    expect(check).not.toHaveBeenCalled();
    await deb.dispose();
    await service.dispose();
  });

  it('backs off on background failure without an editor-blocking error', async () => {
    const { service, check, advance } = fixture();
    check.mockRejectedValueOnce(new Error('offline'));
    await service.check();
    expect(get(service).phase).toBe('idle');
    await service.check();
    expect(check).toHaveBeenCalledTimes(1);
    advance(60 * 60 * 1000);
    await service.check();
    expect(check).toHaveBeenCalledTimes(2);
    await service.dispose();
  });

  it('keeps no-update and missing-target checks nonintrusive', async () => {
    const { service, check, advance } = fixture();
    check.mockResolvedValueOnce(null).mockRejectedValueOnce(new Error('missing target'));
    await service.check(true);
    expect(get(service).phase).toBe('idle');
    advance(24 * 60 * 60 * 1000);
    await service.check();
    expect(get(service).phase).toBe('idle');
    await service.dispose();
  });

  it('does not install after an interrupted or rejected download', async () => {
    const { service, candidate } = fixture();
    vi.mocked(candidate.download).mockRejectedValueOnce(new Error('invalid signature'));
    await service.check();
    await service.download();
    expect(get(service).phase).toBe('error');
    expect(candidate.install).not.toHaveBeenCalled();
    expect(candidate.close).toHaveBeenCalledOnce();
    await service.dispose();
  });
});
