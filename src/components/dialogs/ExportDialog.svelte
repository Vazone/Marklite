<script lang="ts">
  import {
    defaultExportOptions
  } from '../../lib/documentExport';
  import type {
    ExportFormat,
    ExportMarginPreset,
    ExportOptions,
    ExportOrientation,
    ExportPaperSize
  } from '../../lib/tauriApi';
  import ExportProgress from './ExportProgress.svelte';
  import type { ExportProgressView } from '../../lib/exportProgress';
  import ModalShell from './ModalShell.svelte';
  import { translator } from '../../lib/i18n';

  export let open = false;
  export let progress: ExportProgressView | null = null;
  export let busy = false;
  export let documentTitle = '';
  export let onCancel: (() => void) | undefined = undefined;
  export let cancelRequested = false;
  export let onExport: (format: ExportFormat, options: ExportOptions) => void;
  export let onClose: () => void;

  let format: ExportFormat = 'html';
  let options: ExportOptions = { ...defaultExportOptions };

  function update<K extends keyof ExportOptions>(key: K, value: ExportOptions[K]): void {
    options = { ...options, [key]: value };
  }

  function handleSubmit(event: SubmitEvent): void {
    event.preventDefault();
    if (!busy) onExport(format, { ...options });
  }

</script>

{#if open}
  <ModalShell {busy} dialogClass="export-dialog" labelledBy="export-dialog-title" {onClose}>
      <header class="dialog-header">
        <div>
          <h2 id="export-dialog-title">{$translator('export.title')}</h2>
          <p title={documentTitle}>{documentTitle || $translator('export.currentDocument')}</p>
        </div>
        <button type="button" class="ghost-button" disabled={busy} on:click={onClose}>{$translator('common.close')}</button>
      </header>

      <form on:submit={handleSubmit}>
        <div class="export-dialog-body">
          {#if busy && progress}<ExportProgress {progress} />{/if}
          <label>
            <span>{$translator('export.fileFormat')}</span>
            <select bind:value={format} disabled={busy}>
              <option value="html">{$translator('export.format.html')}</option>
              <option value="pdf">{$translator('export.format.pdf')}</option>
              <option value="docx">{$translator('export.format.docx')}</option>
              <option value="png">{$translator('export.format.png')}</option>
              <option value="svg">{$translator('export.format.svg')}</option>
            </select>
          </label>

          {#if format === 'pdf' || format === 'docx'}
            <div class="export-grid">
              <label>
                <span>{$translator('export.paper')}</span>
                <select
                  value={options.paperSize}
                  disabled={busy}
                  on:change={(event) => update('paperSize', event.currentTarget.value as ExportPaperSize)}
                >
                  <option value="a4">A4</option>
                  <option value="letter">Letter</option>
                </select>
              </label>
              <label>
                <span>{$translator('export.orientation')}</span>
                <select
                  value={options.orientation}
                  disabled={busy}
                  on:change={(event) => update('orientation', event.currentTarget.value as ExportOrientation)}
                >
                  <option value="portrait">{$translator('export.orientation.portrait')}</option>
                  <option value="landscape">{$translator('export.orientation.landscape')}</option>
                </select>
              </label>
              <label>
                <span>{$translator('export.margin')}</span>
                <select
                  value={options.margin}
                  disabled={busy}
                  on:change={(event) => update('margin', event.currentTarget.value as ExportMarginPreset)}
                >
                  <option value="narrow">{$translator('export.margin.narrow')}</option>
                  <option value="normal">{$translator('export.margin.normal')}</option>
                  <option value="wide">{$translator('export.margin.wide')}</option>
                </select>
              </label>
            </div>
          {/if}

          {#if format === 'png'}
            <p class="export-note">{$translator('export.pngNote')}</p>
          {/if}
          {#if format === 'svg'}
            <p class="export-note">
              {$translator('export.svgNote')}
            </p>
          {:else}
            <label class="switch-row">
              <span>{$translator('export.includeTitle')}</span>
              <input
                type="checkbox"
                checked={options.includeTitle}
                disabled={busy}
                on:change={(event) => update('includeTitle', event.currentTarget.checked)}
              />
            </label>
            <label class="switch-row">
              <span>{$translator('export.includeImages')}</span>
              <input
                type="checkbox"
                checked={options.includeLocalImages}
                disabled={busy}
                on:change={(event) => update('includeLocalImages', event.currentTarget.checked)}
              />
            </label>
            <p class="export-note">
              {$translator('export.resourceNote')}
            </p>
          {/if}
        </div>

        <footer class="dialog-footer">
          <button type="button" class="ghost-button" disabled={busy && (!onCancel || format !== 'png' || cancelRequested)} on:click={() => busy ? onCancel?.() : onClose()}>{$translator(cancelRequested ? 'export.cancelling' : 'common.cancel')}</button>
          <button type="submit" class="primary-button" disabled={busy}>
            {busy ? $translator('export.exporting') : $translator(format === 'png' ? 'export.exportImages' : 'export.chooseTarget')}
          </button>
        </footer>
      </form>
  </ModalShell>
{/if}

<style>
  :global(.export-dialog) {
    width: min(560px, calc(100vw - 32px));
    overflow: hidden;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-lg);
    background: var(--panel-bg);
    box-shadow: var(--shadow-strong);
  }

  .export-dialog-body {
    display: grid;
    gap: 16px;
    padding: 22px;
  }

  label {
    display: grid;
    gap: 7px;
  }

  label > span {
    color: var(--text-primary);
    font-size: 0.88rem;
    font-weight: 600;
  }

  select {
    width: 100%;
    min-height: 38px;
    padding: 0 10px;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    background: var(--input-bg);
    color: var(--text-primary);
  }

  .export-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 12px;
  }

  .switch-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
  }

  .export-note {
    margin: 0;
    color: var(--text-secondary);
    font-size: 0.82rem;
    line-height: 1.55;
  }

  @media (max-width: 560px) {
    .export-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
