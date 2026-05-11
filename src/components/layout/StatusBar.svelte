<script lang="ts">
  import type { EditorTab } from '../../app/stores/documentStore';
  import type { LayoutMode } from '../../app/stores/uiStore';

  export let tab: EditorTab | undefined;
  export let layoutMode: LayoutMode = 'split';
</script>

<footer class="statusbar">
  <span>{tab?.isDirty ? '未保存' : '已保存'}</span>
  <span>{tab?.path ?? '尚未选择路径'}</span>
  <span>{tab?.stats.wordCount ?? 0} 词</span>
  <span>{tab?.stats.characterCount ?? 0} 字符</span>
  <span>{tab?.stats.lineCount ?? 1} 行</span>
  <span>Ln {tab?.cursorPosition.line ?? 1}, Col {tab?.cursorPosition.column ?? 1}</span>
  <span class="mode">{layoutMode === 'split' ? '分栏' : layoutMode === 'edit' ? '编辑' : '预览'}</span>
</footer>
