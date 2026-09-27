<script lang="ts">
  import { translator } from '../../lib/i18n';
  import type { UpdateView } from './service';

  export let state: UpdateView;
  export let onDownload: () => void;
  export let onInstall: () => void;
  export let onDismiss: () => void;
  export let onSkip: () => void;
  export let onOpenReleases: () => void;

  $: percent = state.phase === 'downloading' && state.totalBytes
    ? Math.min(100, Math.floor((state.downloadedBytes ?? 0) * 100 / state.totalBytes))
    : null;
</script>

{#if state.phase !== 'idle' && state.phase !== 'checking'}
  <aside class="update-banner" role="status" aria-live="polite">
    <div class="update-message">
      {#if state.phase === 'manual'}
        <strong>{$translator('update.manualTitle')}</strong>
        <span>{$translator('update.manualDescription')}</span>
      {:else if state.phase === 'error'}
        <strong>{$translator('update.error')}</strong>
        <span>{state.error}</span>
      {:else if state.phase === 'installed'}
        <strong>{$translator('update.installed')}</strong>
      {:else}
        <strong>{$translator('update.available', { version: state.version ?? '' })}</strong>
        {#if state.notes && state.phase === 'available'}<span class="update-notes">{state.notes}</span>{/if}
        {#if state.phase === 'downloading'}
          <span>{percent === null ? $translator('update.downloading') : $translator('update.progress', { percent })}</span>
          {#if state.totalBytes}
            <progress value={state.downloadedBytes ?? 0} max={state.totalBytes} aria-label={$translator('update.downloading')}></progress>
          {:else}
            <progress aria-label={$translator('update.downloading')}></progress>
          {/if}
        {/if}
        {#if state.phase === 'downloaded'}<span>{$translator('update.ready')}</span>{/if}
        {#if state.phase === 'installing'}<span>{$translator('update.installing')}</span>{/if}
      {/if}
    </div>
    <div class="update-actions">
      {#if state.phase === 'available'}
        <button type="button" on:click={onDownload}>{$translator('update.download')}</button>
        <button type="button" on:click={onSkip}>{$translator('update.skipVersion')}</button>
      {:else if state.phase === 'downloaded'}
        <button type="button" on:click={onInstall}>{$translator('update.install')}</button>
      {:else if state.phase === 'manual'}
        <button type="button" on:click={onOpenReleases}>{$translator('update.openReleases')}</button>
      {/if}
      {#if state.phase !== 'downloading' && state.phase !== 'installing'}
        <button type="button" on:click={onDismiss}>{$translator('common.close')}</button>
      {/if}
    </div>
  </aside>
{/if}

<style>
  .update-banner { display: flex; align-items: center; justify-content: space-between; gap: 1rem; padding: .55rem .9rem; border-bottom: 1px solid var(--border-color, #d5dce0); background: var(--surface, #f6fbfb); }
  .update-message { display: flex; flex-wrap: wrap; align-items: center; gap: .5rem; min-width: 0; font-size: .86rem; }
  .update-notes { max-width: 42rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .update-actions { display: flex; flex-shrink: 0; gap: .4rem; }
  .update-actions button { cursor: pointer; }
  progress { width: 8rem; }
</style>
