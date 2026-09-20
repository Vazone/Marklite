<script lang="ts">
  import { Download, ExternalLink, Trash2 } from 'lucide-svelte';
  import ModalShell from './ModalShell.svelte';
  import { translator } from '../../lib/i18n';
  export let open = false;
  export let onClose: () => void;
  export let onExportDiagnostics: () => void;
  export let onClearDiagnostics: () => void;
  export let onOpenRepository: (url: string) => void;

  const repositoryUrl = 'https://github.com/Vazone/Marklite';
</script>

{#if open}
  <ModalShell dialogClass="about-dialog" ariaLabel={$translator('about.title')} {onClose}>
      <header class="dialog-header">
        <div>
          <h2>MarkLite</h2>
          <p>Rust + Tauri + Svelte + CodeMirror</p>
        </div>
        <button type="button" class="ghost-button" on:click={onClose}>{$translator('common.close')}</button>
      </header>
      <p>
        {$translator('about.description')}
      </p>
      <div class="about-meta">
        <span>{$translator('about.author')}</span>
        <button type="button" on:click={() => onOpenRepository(repositoryUrl)}>
          <ExternalLink size={15} />
          <span>{repositoryUrl}</span>
        </button>
      </div>
      <div class="about-stack">
        <span>Tauri 2</span>
        <span>pulldown-cmark</span>
        <span>CodeMirror 6</span>
        <span>Svelte 5</span>
      </div>
      <div class="about-diagnostics" aria-label={$translator('about.diagnostics')}>
        <p>{$translator('about.diagnosticsPrivacy')}</p>
        <div>
          <button type="button" class="ghost-button" on:click={onExportDiagnostics}>
            <Download size={15} />
            {$translator('about.exportDiagnostics')}
          </button>
          <button type="button" class="ghost-button" on:click={onClearDiagnostics}>
            <Trash2 size={15} />
            {$translator('about.clearDiagnostics')}
          </button>
        </div>
      </div>
  </ModalShell>
{/if}
