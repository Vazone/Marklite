<script lang="ts">
  import { onMount } from 'svelte';
  import {
    FileDown,
    FilePlus2,
    FolderOpen,
    FolderTree,
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
    BookOpen,
    History,
    MoreHorizontal
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
  export let onOpenFolder: () => void = () => {};
  export let showFolderAction = false;
  export let mobile = false;
  export let recoveryAvailable = false;
  export let recoveryCount = 0;
  export let onOpenRecovery: () => void = () => {};
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
  export let moreOpen = false;
  let titleViewport: HTMLSpanElement;
  let titleText: HTMLSpanElement;
  let titleOverflow = 0;

  onMount(() => {
    if (!titleViewport || !titleText || typeof ResizeObserver === 'undefined') return;
    const measure = () => {
      titleOverflow = Math.max(0, Math.ceil(titleText.scrollWidth - titleViewport.clientWidth));
    };
    const observer = new ResizeObserver(measure);
    observer.observe(titleViewport);
    observer.observe(titleText);
    measure();
    return () => observer.disconnect();
  });

  function choose(action: () => void): void {
    moreOpen = false;
    action();
  }
</script>

<header class="titlebar" class:mobile>
  <div class="brand">
    <div class="brand-mark" aria-hidden="true">
      <svg viewBox="0 0 24 24" focusable="false"><path d="M3 20V4h3.5L12 12.2 17.5 4H21v16h-3.3V9.7L12 17l-5.7-7.3V20Z" /></svg>
    </div>
    <div class="brand-copy">
      <strong>MarkLite</strong>
      <span class="brand-title" title={isDirty ? `${title} *` : title} bind:this={titleViewport}>
        <span class="brand-title-text" class:scrolling={mobile && titleOverflow > 2}
          style={`--marquee-distance: ${titleOverflow}px; --marquee-duration: ${Math.min(35, Math.max(8, 4 + titleOverflow / 40))}s`}
          bind:this={titleText}>{isDirty ? `${title} *` : title}</span>
      </span>
    </div>
  </div>

  {#if mobile}
    <nav class="mobile-actions" aria-label={$translator('titlebar.mainToolbar')}>
      <button type="button" title={$translator('titlebar.open')} aria-label={$translator('titlebar.open')} on:click={onOpen}><FolderOpen size={20} /></button>
      <button type="button" title={$translator('titlebar.save')} aria-label={$translator('titlebar.save')} on:click={onSave}><Save size={20} /></button>
      <button type="button" title={layoutMode === 'preview' ? $translator('titlebar.layout.edit') : $translator('titlebar.layout.preview')} aria-label={layoutMode === 'preview' ? $translator('titlebar.layout.edit') : $translator('titlebar.layout.preview')} on:click={() => choose(() => onLayout(layoutMode === 'preview' ? 'edit' : 'preview'))}>
        {#if layoutMode === 'preview'}<Edit3 size={20} />{:else}<Eye size={20} />{/if}
      </button>
      <button type="button" title={$translator('titlebar.more')} aria-label={$translator('titlebar.more')} aria-expanded={moreOpen} on:click={() => (moreOpen = !moreOpen)}><MoreHorizontal size={20} /></button>
    </nav>
    {#if moreOpen}
      <button class="mobile-menu-backdrop" type="button" aria-label={$translator('titlebar.closeMenu')} on:click={() => (moreOpen = false)}></button>
      <nav class="mobile-menu" aria-label={$translator('titlebar.more')}>
        <button type="button" on:click={() => choose(onNew)}><FilePlus2 size={18} />{$translator('titlebar.new')}</button>
        {#if showFolderAction}<button type="button" on:click={() => choose(onOpenFolder)}><FolderTree size={18} />{$translator('workspace.choose')}</button>{/if}
        <button type="button" on:click={() => choose(onSaveAs)}><SaveAll size={18} />{$translator('titlebar.saveAs')}</button>
        <button type="button" on:click={() => choose(onExport)}><FileDown size={18} />{$translator('command.export')}</button>
        <button type="button" on:click={() => choose(onToggleSidebar)}><PanelLeft size={18} />{$translator(sidebarVisible ? 'titlebar.sidebar.collapse' : 'titlebar.sidebar.expand')}</button>
        <button type="button" on:click={() => choose(onFind)}><Search size={18} />{$translator('titlebar.search')}</button>
        <button type="button" on:click={() => choose(onCommandPalette)}><Command size={18} />{$translator('titlebar.commands')}</button>
        <button type="button" on:click={() => choose(onSettings)}><Settings size={18} />{$translator('titlebar.settings')}</button>
        {#if recoveryAvailable}<button type="button" on:click={() => choose(onOpenRecovery)}><History size={18} />{$translator('recovery.available', { count: recoveryCount })}</button>{/if}
        <button type="button" on:click={() => choose(onMarkdownGuide)}><BookOpen size={18} />{$translator('titlebar.guide')}</button>
        <button type="button" on:click={() => choose(onAbout)}><Info size={18} />{$translator('titlebar.about')}</button>
      </nav>
    {/if}
  {:else}
  <nav class="top-actions" aria-label={$translator('titlebar.mainToolbar')}>
    <button type="button" title={`${$translator('titlebar.new')} ${shortcutFor('new')}`} on:click={onNew}><FilePlus2 size={17} /></button>
    <button type="button" title={`${$translator('titlebar.open')} ${shortcutFor('open')}`} on:click={onOpen}><FolderOpen size={17} /></button>
    {#if showFolderAction}
      <button type="button" title={$translator('workspace.choose')} aria-label={$translator('workspace.choose')} on:click={onOpenFolder}><FolderTree size={17} /></button>
    {/if}
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
  {/if}
</header>
