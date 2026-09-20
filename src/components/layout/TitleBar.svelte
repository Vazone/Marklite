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
    Info,
    BookOpen
  } from 'lucide-svelte';
  import type { LayoutMode } from '../../app/stores/uiStore';
  import { shortcutFor } from '../../lib/commands';
  import { translator } from '../../lib/i18n';

  export let title = 'MarkLite';
  export let isDirty = false;
  export let layoutMode: LayoutMode = 'split';
  export let sidebarVisible = true;
  export let onNew: () => void;
  export let onOpen: () => void;
  export let onSave: () => void;
  export let onSaveAs: () => void;
  export let onExport: () => void;
  export let onFind: () => void;
  export let onSettings: () => void;
  export let onToggleSidebar: () => void;
  export let onLayout: (mode: LayoutMode) => void;
  export let onCommandPalette: () => void;
  export let onMarkdownGuide: () => void;
  export let onAbout: () => void;
</script>

<header class="titlebar">
  <div class="brand">
    <div class="brand-mark">M</div>
    <div>
      <strong>MarkLite</strong>
      <span>{isDirty ? `${title} *` : title}</span>
    </div>
  </div>

  <nav class="top-actions" aria-label={$translator('titlebar.mainToolbar')}>
    <button type="button" title={`${$translator('titlebar.new')} ${shortcutFor('new')}`} on:click={onNew}><FilePlus2 size={17} /></button>
    <button type="button" title={`${$translator('titlebar.open')} ${shortcutFor('open')}`} on:click={onOpen}><FolderOpen size={17} /></button>
    <button type="button" title={`${$translator('titlebar.save')} ${shortcutFor('save')}`} on:click={onSave}><Save size={17} /></button>
    <button type="button" title={`${$translator('titlebar.saveAs')} ${shortcutFor('save-as')}`} on:click={onSaveAs}><SaveAll size={17} /></button>
    <button type="button" title={$translator('command.export')} on:click={onExport}><FileDown size={17} /></button>
    <span class="divider"></span>
    <button
      type="button"
      class="sidebar-toggle"
      class:active={sidebarVisible}
      class:restore={!sidebarVisible}
      title={sidebarVisible ? $translator('titlebar.sidebar.collapse') : $translator('titlebar.sidebar.expand')}
      aria-label={sidebarVisible ? $translator('titlebar.sidebar.collapse') : $translator('titlebar.sidebar.expand')}
      on:click={onToggleSidebar}
    >
      <PanelLeft size={17} />
      {#if !sidebarVisible}<span>{$translator('titlebar.sidebar.expand')}</span>{/if}
    </button>
    <button type="button" title={`${$translator('titlebar.search')} ${shortcutFor('find')}`} on:click={onFind}><Search size={17} /></button>
    <button type="button" title={`${$translator('titlebar.commands')} ${shortcutFor('command-palette')}`} on:click={onCommandPalette}><Command size={17} /></button>
    <button type="button" title={`${$translator('titlebar.settings')} ${shortcutFor('settings')}`} on:click={onSettings}><Settings size={17} /></button>
    <button type="button" title={$translator('titlebar.guide')} aria-label={$translator('titlebar.guide')} on:click={onMarkdownGuide}><BookOpen size={17} /></button>
    <button type="button" title={$translator('titlebar.about')} on:click={onAbout}><Info size={17} /></button>
  </nav>

  <div class="layout-switch" aria-label={$translator('titlebar.layout')}>
    <button type="button" class:active={layoutMode === 'edit'} title={$translator('command.layoutEdit')} on:click={() => onLayout('edit')}>
      <Edit3 size={16} />
    </button>
    <button type="button" class:active={layoutMode === 'split'} title={$translator('command.layoutSplit')} on:click={() => onLayout('split')}>
      <Columns2 size={16} />
    </button>
    <button type="button" class:active={layoutMode === 'preview'} title={$translator('command.layoutPreview')} on:click={() => onLayout('preview')}>
      <Eye size={16} />
    </button>
  </div>
</header>
