<script lang="ts">
  import type { MarkdownDiagnostic } from '../../lib/platform/contracts';
  import { localizeError, translator } from '../../lib/i18n';

  export let diagnostics: MarkdownDiagnostic[] = [];
  export let onJumpToLine: (line: number) => void;
</script>

{#if diagnostics.length}
  <aside class="markdown-diagnostics" aria-label={$translator('markdown.diagnostics')}>
    {#each diagnostics as diagnostic}
      <button type="button" on:click={() => onJumpToLine(diagnostic.line)}>
        {$translator('markdown.diagnosticLocation', { line: diagnostic.line, column: diagnostic.column })}:
        {localizeError(diagnostic, $translator)}
      </button>
    {/each}
  </aside>
{/if}

<style>
  .markdown-diagnostics { padding: 0.5rem 0.75rem; max-height: 8rem; overflow: auto; }
  button { display: block; text-align: start; font: inherit; color: inherit; cursor: pointer; }
</style>
