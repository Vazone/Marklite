<script lang="ts">
  import { Copy } from 'lucide-svelte';
  import ModalShell from './ModalShell.svelte';
  import { serializeExportWarningReport, type ExportWarningReport } from '../../lib/exportWarningReport';
  import { translator } from '../../lib/i18n';

  export let report: ExportWarningReport;
  export let onClose: () => void;
  export let onJumpToLine: (line: number) => void;
  export let canJump = false;
  let copyFailed = false;

  async function copyReport() {
    try {
      await navigator.clipboard.writeText(serializeExportWarningReport(report));
      copyFailed = false;
    } catch {
      copyFailed = true;
    }
  }
</script>

<ModalShell dialogClass="export-warnings-dialog" labelledBy="export-warnings-title" {onClose}>
  <header class="dialog-header">
    <div>
      <h2 id="export-warnings-title">{$translator('export.warningTitle')}</h2>
      <p>{report.documentTitle} · {report.format.toUpperCase()}</p>
    </div>
    <button type="button" class="ghost-button" on:click={onClose}>{$translator('common.close')}</button>
  </header>
  <div class="export-warnings-content">
    <p>{$translator('export.warningJob', { jobId: report.jobId, revision: report.contentRevision })}</p>
    <p class="warning-path">{report.sourcePath ?? report.documentTitle} → {report.outputPath}</p>
    <ol>
      {#each report.warnings as warning}
        <li>
          <strong>{warning.code}</strong>
          <p>{warning.message}</p>
          {#if warning.target}<code>{warning.target}</code>{/if}
          {#if warning.line !== null}
            {#if canJump}
              <button type="button" on:click={() => onJumpToLine(warning.line!)}>
                {$translator('export.warningLine', { line: warning.line })}
              </button>
            {:else}
              <span>{$translator('export.warningLine', { line: warning.line })}</span>
            {/if}
          {/if}
          {#if warning.excerpt}<pre>{warning.excerpt}</pre>{/if}
        </li>
      {/each}
    </ol>
    {#if copyFailed}<p role="status">{$translator('export.warningCopyFailed')}</p>{/if}
  </div>
  <footer class="dialog-footer">
    <button type="button" class="ghost-button" on:click={() => void copyReport()}>
      <Copy size={15} /> {$translator('export.warningCopy')}
    </button>
  </footer>
</ModalShell>

<style>
  :global(.export-warnings-dialog) { display: flex; flex-direction: column; width: min(720px, calc(100vw - 36px)); max-height: min(760px, calc(100vh - 40px)); }
  .export-warnings-content { overflow: auto; padding: 16px 20px; overflow-wrap: anywhere; }
  .warning-path { font-size: .85rem; color: var(--text-secondary); }
  ol { padding-left: 26px; }
  li { margin: 12px 0; padding: 10px; border: 1px solid var(--border-color); border-radius: var(--radius-md); }
  li p { margin: 5px 0; }
  li code, li span, li button { display: inline-block; margin: 4px 8px 0 0; }
  li pre { white-space: pre-wrap; margin-bottom: 0; }
</style>
