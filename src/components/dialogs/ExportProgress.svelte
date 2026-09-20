<script lang="ts">
  import type { ExportProgressView } from '../../lib/exportProgress';
  import { translator } from '../../lib/i18n';
  export let progress: ExportProgressView;
  $: work = progress.work;
  // Completing the last unit still leaves validation, commit and cleanup.
  $: counted = work !== null && work.completed < work.total;
</script>

<section class="export-progress" aria-label={$translator('export.progress.title')}>
  <p role="status" aria-live="polite">{$translator(`export.stage.${progress.stage}`)}</p>
  {#if work}
    <p class="count">{$translator(work.kind === 'chapter' ? 'export.progress.chapter' : 'export.progress.part', { index: work.index, total: work.total, completed: work.completed })}</p>
  {/if}
  <progress aria-label={$translator('export.progress.title')} max={work?.total ?? 1} value={progress.status === 'succeeded' ? (work?.total ?? 1) : counted ? work?.completed : undefined}></progress>
</section>

<style>
  .export-progress { display: grid; gap: 6px; }
  p { margin: 0; font-size: .88rem; }
  .count { color: var(--text-secondary); font-size: .82rem; }
  progress { width: 100%; height: 8px; accent-color: var(--accent-color); }
</style>
