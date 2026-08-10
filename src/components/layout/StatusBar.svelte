<script lang="ts">
  import type { EditorTab } from '../../app/stores/documentStore';
  import type { LayoutMode } from '../../app/stores/uiStore';

  export let tab: EditorTab | undefined;
  export let layoutMode: LayoutMode = 'split';
</script>

<footer class="statusbar">
  <span>
    {tab?.loadState === 'loading'
      ? '加载中'
      : tab?.loadState === 'error'
        ? '加载失败'
        : tab?.loadState === 'unloaded'
          ? '未加载'
          : tab?.isDirty
            ? '未保存'
            : '已保存'}
  </span>
  <span>{tab?.path ?? '尚未选择路径'}</span>
  {#if tab?.loadState === 'loaded'}
    <span>{tab.stats.wordCount} 词</span>
    <span>{tab.stats.characterCount} 字符</span>
    <span>{tab.stats.lineCount} 行</span>
    <span>Ln {tab.cursorPosition.line}, Col {tab.cursorPosition.column}</span>
  {/if}
  <span class="mode">{layoutMode === 'split' ? '分栏' : layoutMode === 'edit' ? '编辑' : '预览'}</span>
</footer>
