<script lang="ts">
  import type { DirtyExitDocument } from '../../lib/exitProtection';
  import ModalShell from './ModalShell.svelte';
  import { translator } from '../../lib/i18n';

  export let open = false;
  export let documents: DirtyExitDocument[] = [];
  export let busy = false;
  export let busyLabel = '';
  export let onSave: () => void;
  export let onDiscard: () => void;
  export let onCancel: () => void;

  let cancelButton: HTMLButtonElement;

</script>

{#if open}
  <ModalShell
    {busy}
    role="alertdialog"
    dialogClass="exit-confirmation-dialog"
    backdropClass="exit-confirmation-backdrop"
    labelledBy="exit-confirmation-title"
    describedBy="exit-confirmation-description"
    initialFocus={cancelButton}
    onClose={onCancel}
  >
      <header class="dialog-header">
        <div>
          <h2 id="exit-confirmation-title">{$translator('exit.title')}</h2>
          <p>{$translator('exit.count', { count: documents.length })}</p>
        </div>
      </header>

      <div class="exit-confirmation-body" id="exit-confirmation-description">
        <p>{$translator('exit.description')}</p>
        <ul aria-label={$translator('exit.documents')}>
          {#each documents.slice(0, 8) as document}
            <li title={document.path ?? document.title}>
              <strong>{document.title}</strong>
              {#if document.path}
                <small>{document.path}</small>
              {:else}
                <small>{$translator('exit.noLocation')}</small>
              {/if}
            </li>
          {/each}
        </ul>
        {#if documents.length > 8}
          <p class="exit-confirmation-more">{$translator('common.andMore', { count: documents.length - 8 })}</p>
        {/if}
        {#if busy}
          <p class="exit-confirmation-progress" aria-live="polite">{busyLabel}</p>
        {:else}
          <p class="exit-confirmation-warning">{$translator('exit.discardWarning')}</p>
        {/if}
      </div>

      <footer class="dialog-footer exit-confirmation-actions">
        <button bind:this={cancelButton} type="button" class="ghost-button" disabled={busy} on:click={onCancel}>
          {$translator('exit.keepOpen')}
        </button>
        <div>
          <button type="button" class="danger-button" disabled={busy} on:click={onDiscard}>{$translator('exit.discard')}</button>
          <button type="button" class="primary-button" disabled={busy} on:click={onSave}>
            {busy ? $translator('exit.processing') : $translator('exit.saveAndExit')}
          </button>
        </div>
      </footer>
  </ModalShell>
{/if}
