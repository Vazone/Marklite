<script lang="ts">
  import { onMount } from 'svelte';
  import { Clock3, FileText, FolderOpen, Info, ListTree, PanelLeftClose, Trash2 } from 'lucide-svelte';
  import type { SidebarDocumentView } from '../../app/stores/documentStore';
  import type { RecentFileDto } from '../../lib/tauriApi';
  import type { SidebarTab } from '../../app/stores/uiStore';
  import { clampContextMenuPosition, type ContextMenuPosition } from '../../lib/contextMenuPosition';
  import { isSameFilePath } from '../../lib/filePathIdentity';
  import { outlineItemAtLine } from '../../lib/outlineSelection';
  import { currentLanguage, translator } from '../../lib/i18n';

  type RecentContextMenu = ContextMenuPosition & {
    path: string;
  };

  export let activeSidebarTab: SidebarTab = 'recent';
  export let recentFiles: RecentFileDto[] = [];
  export let tab: SidebarDocumentView | undefined;
  export let onTabChange: (tab: SidebarTab) => void;
  export let onOpenRecent: (path: string) => void;
  export let onRemoveRecent: (path: string) => void;
  export let onRevealRecent: (path: string) => void;
  export let onJumpToLine: (line: number) => void;
  export let onCollapse: () => void;

  let contextMenu: RecentContextMenu | null = null;

  $: currentLine = tab?.scrollPosition.line ?? tab?.cursorPosition.line ?? 1;
  $: currentOutline = outlineItemAtLine(tab?.outline ?? [], currentLine);
  $: currentRecentIndex = tab?.path
    ? recentFiles.findIndex((file) => isSameFilePath(file.path, tab?.path))
    : -1;

  onMount(() => {
    const close = () => {
      contextMenu = null;
    };
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') close();
    };

    window.addEventListener('click', close);
    window.addEventListener('keydown', closeOnEscape);
    return () => {
      window.removeEventListener('click', close);
      window.removeEventListener('keydown', closeOnEscape);
    };
  });

  function openRecentContext(event: MouseEvent, path: string) {
    event.preventDefault();
    const position = clampContextMenuPosition(
      event.clientX,
      event.clientY,
      220,
      48,
      window.innerWidth,
      window.innerHeight
    );
    contextMenu = {
      ...position,
      path
    };
  }

  function revealContextPath() {
    if (!contextMenu) return;
    onRevealRecent(contextMenu.path);
    contextMenu = null;
  }

  function isCurrentOutline(line: number) {
    return currentOutline?.line === line;
  }
</script>

<aside class="sidebar">
  <div class="sidebar-tabs">
    <button type="button" class:active={activeSidebarTab === 'recent'} title={$translator('sidebar.recent')} on:click={() => onTabChange('recent')}>
      <Clock3 size={16} />
    </button>
    <button type="button" class:active={activeSidebarTab === 'outline'} title={$translator('sidebar.outline')} on:click={() => onTabChange('outline')}>
      <ListTree size={16} />
    </button>
    <button type="button" class:active={activeSidebarTab === 'info'} title={$translator('sidebar.info')} on:click={() => onTabChange('info')}>
      <Info size={16} />
    </button>
    <button type="button" class="sidebar-collapse-button" title={$translator('sidebar.collapse')} aria-label={$translator('sidebar.collapse')} on:click={onCollapse}>
      <PanelLeftClose size={16} />
    </button>
  </div>

  {#if activeSidebarTab === 'recent'}
    <section class="sidebar-section">
      <h2>{$translator('sidebar.recent')}</h2>
      {#if recentFiles.length}
        <div class="recent-list" role="list">
          {#each recentFiles as file, index}
            <div
              class="recent-item"
              class:current={index === currentRecentIndex}
              role="listitem"
              title={file.path}
              on:contextmenu={(event) => openRecentContext(event, file.path)}
            >
              <button
                type="button"
                class="recent-main"
                aria-current={index === currentRecentIndex ? 'page' : undefined}
                on:click={() => onOpenRecent(file.path)}
              >
                <FileText class="recent-file-icon" size={15} aria-hidden="true" />
                <span class="recent-file-text">
                  <strong>{file.title}</strong>
                  <small>{new Date(file.lastOpenedAt).toLocaleString($currentLanguage)}</small>
                </span>
              </button>
              <button type="button" class="icon-danger" title={$translator('sidebar.removeRecent')} on:click={() => onRemoveRecent(file.path)}>
                <Trash2 class="recent-action-icon" size={14} aria-hidden="true" />
              </button>
            </div>
          {/each}
        </div>
      {:else}
        <p class="muted">{$translator('sidebar.recentEmpty')}</p>
      {/if}
    </section>
  {:else if activeSidebarTab === 'outline'}
    <section class="sidebar-section">
      <h2>{$translator('sidebar.outline')}</h2>
      {#if tab?.outline.length}
        <div class="outline-list">
          {#each tab.outline as item}
            <button
              type="button"
              class="outline-item"
              class:current={isCurrentOutline(item.line)}
              style:padding-left={`${8 + (item.level - 1) * 14}px`}
              on:click={() => onJumpToLine(item.line)}
            >
              <span>H{item.level}</span>
              {item.title}
            </button>
          {/each}
        </div>
      {:else}
        <p class="muted">{$translator('sidebar.outlineEmpty')}</p>
      {/if}
    </section>
  {:else}
    <section class="sidebar-section">
      <h2>{$translator('sidebar.info')}</h2>
      <dl class="info-list">
        <div><dt>{$translator('sidebar.fileName')}</dt><dd>{tab?.title ?? 'Untitled.md'}</dd></div>
        <div><dt>{$translator('sidebar.path')}</dt><dd>{tab?.path ?? $translator('common.unsaved')}</dd></div>
        <div><dt>{$translator('sidebar.position')}</dt><dd>{currentOutline ? $translator('sidebar.positionHeading', { title: currentOutline.title, line: currentLine }) : $translator('common.line', { line: currentLine })}</dd></div>
        <div><dt>{$translator('sidebar.size')}</dt><dd>{tab?.fileSize != null ? `${(tab.fileSize / 1024).toFixed(1)} KB` : '-'}</dd></div>
        <div><dt>{$translator('sidebar.lastSaved')}</dt><dd>{tab?.lastSavedAt ? new Date(tab.lastSavedAt).toLocaleString($currentLanguage) : '-'}</dd></div>
        <div><dt>{$translator('sidebar.words')}</dt><dd>{tab?.stats.wordCount ?? 0}</dd></div>
        <div><dt>{$translator('sidebar.characters')}</dt><dd>{tab?.stats.characterCount ?? 0}</dd></div>
        <div><dt>{$translator('sidebar.headings')}</dt><dd>{tab?.stats.headingCount ?? 0}</dd></div>
        <div><dt>{$translator('sidebar.links')}</dt><dd>{tab?.stats.linkCount ?? 0}</dd></div>
        <div><dt>{$translator('sidebar.images')}</dt><dd>{tab?.stats.imageCount ?? 0}</dd></div>
      </dl>
    </section>
  {/if}

  {#if contextMenu}
    <div
      class="sidebar-context-menu"
      style:left={`${contextMenu.x}px`}
      style:top={`${contextMenu.y}px`}
      style:width={`${contextMenu.width}px`}
      style:max-height={`${contextMenu.maxHeight}px`}
      style:overflow-y="auto"
      role="menu"
      tabindex="-1"
    >
      <button type="button" role="menuitem" on:click={revealContextPath}>
        <FolderOpen size={15} />
        <span>{$translator('sidebar.reveal')}</span>
      </button>
    </div>
  {/if}
</aside>
