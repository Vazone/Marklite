import { writable } from 'svelte/store';

type DownloadEvent =
  | { event: 'Started'; data: { contentLength?: number } }
  | { event: 'Progress'; data: { chunkLength: number } }
  | { event: 'Finished' };

export type UpdateCandidate = {
  version: string;
  body?: string;
  download: (onProgress: (event: DownloadEvent) => void) => Promise<void>;
  install: () => Promise<void>;
  close: () => Promise<void>;
};

export type UpdateAdapter = {
  bundleType: () => Promise<string>;
  check: () => Promise<UpdateCandidate | null>;
};

export type UpdatePhase = 'idle' | 'checking' | 'available' | 'downloading' | 'downloaded' | 'installing' | 'installed' | 'manual' | 'error';
export type UpdateView = {
  phase: UpdatePhase;
  version?: string;
  notes?: string;
  downloadedBytes?: number;
  totalBytes?: number;
  error?: string;
};

type Preferences = Pick<Storage, 'getItem' | 'setItem'>;
type Dependencies = {
  adapter: UpdateAdapter;
  preferences: Preferences;
  autoEnabled: () => boolean;
  prepareInstall: () => Promise<boolean>;
  reportError: (error: unknown) => string;
  now?: () => number;
};

const lastCheckKey = 'marklite:update:last-check';
const failureCountKey = 'marklite:update:failures';
const skippedVersionKey = 'marklite:update:skipped-version';
const day = 24 * 60 * 60 * 1000;

export function createUpdateService(deps: Dependencies) {
  const store = writable<UpdateView>({ phase: 'idle' });
  let view: UpdateView = { phase: 'idle' };
  let candidate: UpdateCandidate | null = null;
  let disposed = false;
  let active = false;
  let bundleType: string | null = null;
  const now = deps.now ?? Date.now;
  const read = (key: string) => { try { return deps.preferences.getItem(key); } catch { return null; } };
  const write = (key: string, value: string) => { try { deps.preferences.setItem(key, value); } catch { /* optional scheduling cache */ } };
  const publish = (next: UpdateView) => { if (!disposed) { view = next; store.set(next); } };
  const release = async () => {
    const current = candidate;
    candidate = null;
    if (current) {
      try { await current.close(); } catch { /* A stale plugin resource must not mask an update failure. */ }
    }
  };

  async function check(manual = false): Promise<void> {
    if (disposed || active || view.phase === 'installing' || view.phase === 'downloaded') return;
    if (!manual && !deps.autoEnabled()) return;
    if (!manual) {
      const last = Number(read(lastCheckKey) ?? 0);
      const failures = Number(read(failureCountKey) ?? 0);
      const interval = failures > 0 ? Math.min(day, 60 * 60 * 1000 * 2 ** Math.min(failures - 1, 5)) : day;
      if (Number.isFinite(last) && last > 0 && now() - last < interval) return;
    }
    active = true;
    publish({ phase: 'checking' });
    try {
      bundleType ??= await deps.adapter.bundleType();
      if (disposed) return;
      if (!['nsis', 'app', 'appimage'].includes(bundleType)) {
        publish(manual && (bundleType === 'deb' || bundleType === 'rpm') ? { phase: 'manual' } : { phase: 'idle' });
        return;
      }
      await release();
      const result = await deps.adapter.check();
      if (disposed) { await result?.close(); return; }
      write(lastCheckKey, String(now()));
      write(failureCountKey, '0');
      if (!result) { publish({ phase: 'idle' }); return; }
      if (!manual && read(skippedVersionKey) === result.version) {
        await result.close();
        publish({ phase: 'idle' });
        return;
      }
      candidate = result;
      publish({ phase: 'available', version: result.version, notes: result.body });
    } catch (error) {
      write(lastCheckKey, String(now()));
      write(failureCountKey, String(Math.min(6, Number(read(failureCountKey) ?? 0) + 1)));
      publish(manual ? { phase: 'error', error: deps.reportError(error) } : { phase: 'idle' });
    } finally { active = false; }
  }

  async function download(): Promise<void> {
    if (disposed || active || !candidate || view.phase !== 'available') return;
    active = true;
    const current = candidate;
    let downloadedBytes = 0;
    let totalBytes: number | undefined;
    publish({ ...view, phase: 'downloading', downloadedBytes });
    try {
      await current.download(event => {
        if (candidate !== current || disposed) return;
        if (event.event === 'Started') totalBytes = event.data.contentLength && event.data.contentLength > 0 ? event.data.contentLength : undefined;
        if (event.event === 'Progress') downloadedBytes += Math.max(0, event.data.chunkLength);
        publish({ phase: 'downloading', version: current.version, notes: current.body, downloadedBytes, totalBytes });
      });
      if (candidate === current) publish({ phase: 'downloaded', version: current.version, notes: current.body, downloadedBytes, totalBytes });
    } catch (error) {
      await release();
      publish({ phase: 'error', error: deps.reportError(error) });
    } finally { active = false; }
  }

  async function install(): Promise<boolean> {
    if (disposed || active || !candidate || view.phase !== 'downloaded') return false;
    active = true;
    const current = candidate;
    try {
      if (!await deps.prepareInstall() || candidate !== current || disposed) return false;
      publish({ ...view, phase: 'installing' });
      await current.install();
      publish({ phase: 'installed', version: current.version });
      await release();
      return true;
    } catch (error) {
      await release();
      publish({ phase: 'error', error: deps.reportError(error) });
      return false;
    } finally { active = false; }
  }

  async function dismiss(skipVersion = false): Promise<void> {
    if (active || disposed) return;
    if (skipVersion && candidate) write(skippedVersionKey, candidate.version);
    await release();
    publish({ phase: 'idle' });
  }

  return {
    subscribe: store.subscribe,
    check, download, install, dismiss,
    async dispose() { disposed = true; await release(); }
  };
}
