<script lang="ts">
  import type { StatusDocumentView } from '../../app/stores/documentStore';
  import type { LayoutMode } from '../../app/stores/uiStore';
  import { translator } from '../../lib/i18n';

  export let tab: StatusDocumentView | undefined;
  export let layoutMode: LayoutMode = 'split';
  export let mobile = false;
</script>

<footer class="statusbar" class:mobile>
  <span class="status-state">
    {tab?.loadState === 'loading'
      ? $translator('status.loading')
      : tab?.loadState === 'error'
        ? $translator('status.loadFailed')
        : tab?.loadState === 'unloaded'
          ? $translator('status.unloaded')
          : tab?.isDirty
            ? $translator('status.unsaved')
            : $translator('status.saved')}
  </span>
  <span class="status-document" title={tab?.path ?? tab?.title ?? ''}>{mobile ? tab?.title ?? $translator('status.noPath') : tab?.path ?? $translator('status.noPath')}</span>
  {#if !mobile && tab?.loadState === 'loaded'}
    <span>{$translator('status.words', { count: tab.stats.wordCount })}</span>
    <span>{$translator('status.characters', { count: tab.stats.characterCount })}</span>
    <span>{$translator('status.lines', { count: tab.stats.lineCount })}</span>
    <span>{$translator('status.lineColumn', { line: tab.cursorPosition.line, column: tab.cursorPosition.column })}</span>
  {/if}
  <span class="mode">{layoutMode === 'split' ? $translator('status.layout.split') : layoutMode === 'edit' ? $translator('status.layout.edit') : $translator('status.layout.preview')}</span>
</footer>
