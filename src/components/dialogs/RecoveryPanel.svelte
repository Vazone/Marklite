<script lang="ts">
  import ModalShell from './ModalShell.svelte';
  import { translator } from '../../lib/i18n';
  import type { RecoveryEntry, RecoveryInventory } from '../../lib/platform/recovery';
  import type { RecoverySourceStatus } from '../../app/controllers/recoveryController';

  export let inventory: RecoveryInventory;
  export let onRestore: (entry: RecoveryEntry) => Promise<void>;
  export let onDiscard: (entry: RecoveryEntry) => Promise<void>;
  export let onInspect: (entry: RecoveryEntry) => Promise<RecoverySourceStatus>;
  export let onError: (error: unknown) => void;
  export let open = false;
  export let showNotice = true;
  let busy = false;
  let selected: RecoveryEntry | null = null;
  let sourceStatus: RecoverySourceStatus | null = null;

  function location(entry: RecoveryEntry) {
    const resource = entry.resource;
    return !resource ? '' : 'path' in resource ? resource.path : resource.uri;
  }
  async function select(entry: RecoveryEntry) {
    busy = true; selected = entry; sourceStatus = null;
    try { sourceStatus = await onInspect(entry); } catch (error) { onError(error); }
    finally { busy = false; }
  }
  async function act(action: 'restore' | 'discard') {
    if (!selected || busy) return;
    busy = true;
    try {
      await (action === 'restore' ? onRestore(selected) : onDiscard(selected));
      selected = null; sourceStatus = null;
      if (!inventory.entries.length && !inventory.issues.length) open = false;
    } catch (error) { onError(error); }
    finally { busy = false; }
  }
</script>

{#if showNotice && (inventory.entries.length || inventory.issues.length)}
  <button type="button" class="recovery-notice ghost-button" on:click={() => (open = true)}>
    {$translator('recovery.available', { count: inventory.entries.length })}
  </button>
{/if}
{#if open}
  <ModalShell {busy} labelledBy="recovery-title" dialogClass="recovery-dialog" onClose={() => (open = false)}>
    <header class="dialog-header"><h2 id="recovery-title">{$translator('recovery.title')}</h2></header>
    <div class="recovery-body">
      <p>{$translator('recovery.description')}</p>
      {#each inventory.entries as entry (entry.receipt.id)}
        <button class="ghost-button recovery-entry" disabled={busy} on:click={() => void select(entry)}>
          <strong>{entry.title}</strong><span>{new Date(entry.receipt.updatedAt).toLocaleString()}</span>
        </button>
      {/each}
      {#if selected}
        <section aria-live="polite">
          <h3>{selected.title}</h3>
          {#if location(selected)}<p class="recovery-path">{location(selected)}</p>{/if}
          {#if sourceStatus}<p>{$translator(`recovery.${sourceStatus}`)}</p>{/if}
          {#if selected.stale}<p>{$translator('recovery.stale')}</p>{/if}
          <div class="recovery-actions">
            <button type="button" class="primary-button" disabled={busy || !sourceStatus} on:click={() => void act('restore')}>{$translator('recovery.restore')}</button>
            <button type="button" class="danger-button" disabled={busy} on:click={() => void act('discard')}>{$translator('recovery.discard')}</button>
          </div>
        </section>
      {/if}
      {#each inventory.issues as issue}<p role="status">{$translator('recovery.invalid')} ({issue.error.code})</p>{/each}
    </div>
    <footer class="dialog-footer"><button class="ghost-button" disabled={busy} on:click={() => (open = false)}>{$translator('common.close')}</button></footer>
  </ModalShell>
{/if}

<style>
  .recovery-notice { position: fixed; right: 16px; bottom: 36px; z-index: 20; background: var(--panel-bg); }
  .recovery-body { padding: 0 24px 20px; max-height: 65vh; overflow: auto; }
  .recovery-entry { display: flex; width: 100%; justify-content: space-between; gap: 16px; text-align: left; }
  .recovery-path { overflow-wrap: anywhere; }
  .recovery-actions { display: flex; gap: 12px; }
</style>
