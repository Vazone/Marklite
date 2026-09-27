<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { localizeError, translator } from '../../lib/i18n';
  import { api, pickDiagramPackPath, toAppError } from '../../lib/tauriApi';
  import type { DiagramRuntimeStatus } from '../../lib/tauriApi';

  export let onChanged: () => void | Promise<void> = () => undefined;

  let status: DiagramRuntimeStatus | null = null;
  let busy = false;
  let error = '';
  let mounted = true;

  onMount(() => { void refreshStatus(); });
  onDestroy(() => { mounted = false; });

  async function refreshStatus() {
    if (busy) return;
    busy = true;
    error = '';
    try {
      const result = await api.getDiagramRuntimeStatus();
      if (mounted) status = result;
    } catch (cause) {
      if (mounted) error = localizeError(toAppError(cause));
    } finally {
      if (mounted) busy = false;
    }
  }

  async function install() {
    if (busy) return;
    busy = true;
    error = '';
    try {
      const path = await pickDiagramPackPath();
      if (!path) return;
      const result = await api.installDiagramRuntime(path);
      if (mounted) status = result;
      await onChanged();
    } catch (cause) {
      if (mounted) error = localizeError(toAppError(cause));
    } finally {
      if (mounted) busy = false;
    }
  }

  async function uninstall() {
    if (busy) return;
    busy = true;
    error = '';
    try {
      const result = await api.uninstallDiagramRuntime();
      if (mounted) status = result;
      await onChanged();
    } catch (cause) {
      if (mounted) error = localizeError(toAppError(cause));
    } finally {
      if (mounted) busy = false;
    }
  }
</script>

<section class="diagram-runtime-control" aria-label={$translator('settings.diagramPack.title')}>
  <h3>{$translator('settings.diagramPack.title')}</h3>
  <p aria-live="polite">
    {#if status}
      {$translator(status.installed ? 'settings.diagramPack.installed' : 'settings.diagramPack.missing')}
    {:else if busy}
      {$translator('settings.diagramPack.checking')}
    {:else}
      {$translator('settings.diagramPack.unknown')}
    {/if}
  </p>
  <div class="diagram-runtime-actions">
    <button type="button" class="ghost-button" disabled={busy} on:click={install}>{$translator('settings.diagramPack.install')}</button>
    {#if status?.installed}
      <button type="button" class="ghost-button" disabled={busy} on:click={uninstall}>{$translator('settings.diagramPack.uninstall')}</button>
    {/if}
    {#if !status && !busy}
      <button type="button" class="ghost-button" on:click={refreshStatus}>{$translator('settings.diagramPack.retry')}</button>
    {/if}
  </div>
  {#if error}<p role="alert">{error}</p>{/if}
</section>
