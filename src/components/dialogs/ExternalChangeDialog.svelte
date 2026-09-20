<script lang="ts">
  import ModalShell from './ModalShell.svelte';
  import { translator } from '../../lib/i18n';

  export let open = false;
  export let title = '';
  export let path = '';
  export let busy = false;
  export let onReload: () => void;
  export let onSaveCopy: () => void;
  export let onOverwrite: () => void;
  export let onCancel: () => void;

  let cancelButton: HTMLButtonElement;
</script>

{#if open}
  <ModalShell
    {busy}
    role="alertdialog"
    dialogClass="external-change-dialog"
    labelledBy="external-change-title"
    describedBy="external-change-description"
    initialFocus={cancelButton}
    onClose={onCancel}
  >
    <header class="dialog-header">
      <div>
        <h2 id="external-change-title">{$translator('conflict.title')}</h2>
        <p title={path}>{title}</p>
      </div>
    </header>
    <div class="external-change-body" id="external-change-description">
      <p>{$translator('conflict.description')}</p>
      <p class="external-change-path" title={path}>{path}</p>
      <p class="external-change-warning">{$translator('conflict.warning')}</p>
    </div>
    <footer class="dialog-footer external-change-actions">
      <button bind:this={cancelButton} type="button" class="ghost-button" disabled={busy} on:click={onCancel}>{$translator('common.cancel')}</button>
      <div>
        <button type="button" class="ghost-button" disabled={busy} on:click={onReload}>{$translator('conflict.reload')}</button>
        <button type="button" class="ghost-button" disabled={busy} on:click={onSaveCopy}>{$translator('conflict.saveCopy')}</button>
        <button type="button" class="danger-button" disabled={busy} on:click={onOverwrite}>{$translator('conflict.overwrite')}</button>
      </div>
    </footer>
  </ModalShell>
{/if}
