<script lang="ts">
  import {
    FileDown,
    FilePlus2,
    FolderOpen,
    PanelLeft,
    Save,
    SaveAll,
    Search,
    Settings,
    Columns2,
    Edit3,
    Eye,
    Command,
    Info
  } from 'lucide-svelte';
  import type { LayoutMode } from '../../app/stores/uiStore';

  export let title = 'MarkLite';
  export let isDirty = false;
  export let layoutMode: LayoutMode = 'split';
  export let sidebarVisible = true;
  export let onNew: () => void = () => {};
  export let onOpen: () => void = () => {};
  export let onSave: () => void = () => {};
  export let onSaveAs: () => void = () => {};
  export let onExportHtml: () => void = () => {};
  export let onFind: () => void = () => {};
  export let onSettings: () => void = () => {};
  export let onToggleSidebar: () => void = () => {};
  export let onLayout: (mode: LayoutMode) => void = () => {};
  export let onCommandPalette: () => void = () => {};
  export let onAbout: () => void = () => {};
</script>

<header class="titlebar">
  <div class="brand">
    <div class="brand-mark">M</div>
    <div>
      <strong>MarkLite</strong>
      <span>{isDirty ? `${title} *` : title}</span>
    </div>
  </div>

  <nav class="top-actions" aria-label="主工具栏">
    <button type="button" title="新建 Ctrl+N" on:click={onNew}><FilePlus2 size={17} /></button>
    <button type="button" title="打开 Ctrl+O" on:click={onOpen}><FolderOpen size={17} /></button>
    <button type="button" title="保存 Ctrl+S" on:click={onSave}><Save size={17} /></button>
    <button type="button" title="另存为 Ctrl+Shift+S" on:click={onSaveAs}><SaveAll size={17} /></button>
    <button type="button" title="导出 HTML" on:click={onExportHtml}><FileDown size={17} /></button>
    <span class="divider"></span>
    <button type="button" class:active={sidebarVisible} title="侧边栏" on:click={onToggleSidebar}><PanelLeft size={17} /></button>
    <button type="button" title="查找 Ctrl+F" on:click={onFind}><Search size={17} /></button>
    <button type="button" title="命令面板 Ctrl+P" on:click={onCommandPalette}><Command size={17} /></button>
    <button type="button" title="设置 Ctrl+," on:click={onSettings}><Settings size={17} /></button>
    <button type="button" title="关于" on:click={onAbout}><Info size={17} /></button>
  </nav>

  <div class="layout-switch" aria-label="布局模式">
    <button type="button" class:active={layoutMode === 'edit'} title="编辑模式" on:click={() => onLayout('edit')}>
      <Edit3 size={16} />
    </button>
    <button type="button" class:active={layoutMode === 'split'} title="分栏模式" on:click={() => onLayout('split')}>
      <Columns2 size={16} />
    </button>
    <button type="button" class:active={layoutMode === 'preview'} title="预览模式" on:click={() => onLayout('preview')}>
      <Eye size={16} />
    </button>
  </div>
</header>
