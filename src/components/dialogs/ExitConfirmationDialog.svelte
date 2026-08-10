<script lang="ts">
  import type { DirtyExitDocument } from '../../lib/exitProtection';

  export let open = false;
  export let documents: DirtyExitDocument[] = [];
  export let busy = false;
  export let busyLabel = '';
  export let onSave: () => void = () => {};
  export let onDiscard: () => void = () => {};
  export let onCancel: () => void = () => {};

  let cancelButton: HTMLButtonElement;

  $: if (open && !busy) {
    window.setTimeout(() => cancelButton?.focus(), 0);
  }

  function cancelIfAllowed(): void {
    if (!busy) onCancel();
  }

  function handleKeydown(event: KeyboardEvent): void {
    event.stopPropagation();
    if (event.key === 'Escape' && !busy) {
      event.preventDefault();
      onCancel();
    }
  }
</script>

{#if open}
  <div class="modal-backdrop exit-confirmation-backdrop" role="presentation" on:click={cancelIfAllowed}>
    <div
      class="exit-confirmation-dialog"
      role="alertdialog"
      aria-modal="true"
      aria-labelledby="exit-confirmation-title"
      aria-describedby="exit-confirmation-description"
      aria-busy={busy}
      tabindex="-1"
      on:click|stopPropagation
      on:keydown={handleKeydown}
    >
      <header class="dialog-header">
        <div>
          <h2 id="exit-confirmation-title">保存未保存的更改？</h2>
          <p>{documents.length} 个文档包含尚未保存的内容</p>
        </div>
      </header>

      <div class="exit-confirmation-body" id="exit-confirmation-description">
        <p>选择“保存并退出”会依次保存下列文档。未命名文档会要求选择保存位置。</p>
        <ul aria-label="未保存文档">
          {#each documents.slice(0, 8) as document}
            <li title={document.path ?? document.title}>
              <strong>{document.title}</strong>
              {#if document.path}
                <small>{document.path}</small>
              {:else}
                <small>尚未选择保存位置</small>
              {/if}
            </li>
          {/each}
        </ul>
        {#if documents.length > 8}
          <p class="exit-confirmation-more">以及另外 {documents.length - 8} 个文档</p>
        {/if}
        {#if busy}
          <p class="exit-confirmation-progress" aria-live="polite">{busyLabel}</p>
        {:else}
          <p class="exit-confirmation-warning">选择“不保存”会丢弃这些未保存的更改。</p>
        {/if}
      </div>

      <footer class="dialog-footer exit-confirmation-actions">
        <button bind:this={cancelButton} type="button" class="ghost-button" disabled={busy} on:click={onCancel}>
          取消
        </button>
        <div>
          <button type="button" class="danger-button" disabled={busy} on:click={onDiscard}>不保存</button>
          <button type="button" class="primary-button" disabled={busy} on:click={onSave}>
            {busy ? '处理中…' : '保存并退出'}
          </button>
        </div>
      </footer>
    </div>
  </div>
{/if}
