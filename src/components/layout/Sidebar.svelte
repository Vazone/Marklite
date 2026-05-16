<script lang="ts">
  import { onMount } from 'svelte';
  import { Clock3, FileText, FolderOpen, Info, ListTree, Trash2 } from 'lucide-svelte';
  import type { EditorTab } from '../../app/stores/documentStore';
  import type { RecentFileDto } from '../../lib/tauriApi';
  import type { SidebarTab } from '../../app/stores/uiStore';

  type RecentContextMenu = {
    x: number;
    y: number;
    path: string;
  };

  export let activeSidebarTab: SidebarTab = 'recent';
  export let recentFiles: RecentFileDto[] = [];
  export let tab: EditorTab | undefined;
  export let onTabChange: (tab: SidebarTab) => void = () => {};
  export let onOpenRecent: (path: string) => void = () => {};
  export let onRemoveRecent: (path: string) => void = () => {};
  export let onRevealRecent: (path: string) => void = () => {};
  export let onJumpToLine: (line: number) => void = () => {};

  let contextMenu: RecentContextMenu | null = null;

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
    const menuWidth = 220;
    const menuHeight = 48;
    contextMenu = {
      x: Math.min(event.clientX, window.innerWidth - menuWidth - 8),
      y: Math.min(event.clientY, window.innerHeight - menuHeight - 8),
      path
    };
  }

  function revealContextPath() {
    if (!contextMenu) return;
    onRevealRecent(contextMenu.path);
    contextMenu = null;
  }
</script>

<aside class="sidebar">
  <div class="sidebar-tabs">
    <button type="button" class:active={activeSidebarTab === 'recent'} title="最近文件" on:click={() => onTabChange('recent')}>
      <Clock3 size={16} />
    </button>
    <button type="button" class:active={activeSidebarTab === 'outline'} title="文档大纲" on:click={() => onTabChange('outline')}>
      <ListTree size={16} />
    </button>
    <button type="button" class:active={activeSidebarTab === 'info'} title="文档信息" on:click={() => onTabChange('info')}>
      <Info size={16} />
    </button>
  </div>

  {#if activeSidebarTab === 'recent'}
    <section class="sidebar-section">
      <h2>最近文件</h2>
      {#if recentFiles.length}
        <div class="recent-list" role="list">
          {#each recentFiles as file}
            <div class="recent-item" role="listitem" title={file.path} on:contextmenu={(event) => openRecentContext(event, file.path)}>
              <button type="button" class="recent-main" on:click={() => onOpenRecent(file.path)}>
                <FileText size={15} />
                <span>
                  <strong>{file.title}</strong>
                  <small>{new Date(file.lastOpenedAt).toLocaleString()}</small>
                </span>
              </button>
              <button type="button" class="icon-danger" title="移除记录" on:click={() => onRemoveRecent(file.path)}>
                <Trash2 size={14} />
              </button>
            </div>
          {/each}
        </div>
      {:else}
        <p class="muted">打开文件后会显示在这里。</p>
      {/if}
    </section>
  {:else if activeSidebarTab === 'outline'}
    <section class="sidebar-section">
      <h2>文档大纲</h2>
      {#if tab?.outline.length}
        <div class="outline-list">
          {#each tab.outline as item}
            <button
              type="button"
              class="outline-item"
              style:padding-left={`${8 + (item.level - 1) * 14}px`}
              on:click={() => onJumpToLine(item.line)}
            >
              <span>H{item.level}</span>
              {item.title}
            </button>
          {/each}
        </div>
      {:else}
        <p class="muted">当前文档没有标题。</p>
      {/if}
    </section>
  {:else}
    <section class="sidebar-section">
      <h2>文档信息</h2>
      <dl class="info-list">
        <div><dt>文件名</dt><dd>{tab?.title ?? 'Untitled.md'}</dd></div>
        <div><dt>路径</dt><dd>{tab?.path ?? '未保存'}</dd></div>
        <div><dt>大小</dt><dd>{tab?.fileSize ? `${(tab.fileSize / 1024).toFixed(1)} KB` : '-'}</dd></div>
        <div><dt>最后保存</dt><dd>{tab?.lastSavedAt ? new Date(tab.lastSavedAt).toLocaleString() : '-'}</dd></div>
        <div><dt>词数</dt><dd>{tab?.stats.wordCount ?? 0}</dd></div>
        <div><dt>字符</dt><dd>{tab?.stats.characterCount ?? 0}</dd></div>
        <div><dt>标题</dt><dd>{tab?.stats.headingCount ?? 0}</dd></div>
        <div><dt>链接</dt><dd>{tab?.stats.linkCount ?? 0}</dd></div>
        <div><dt>图片</dt><dd>{tab?.stats.imageCount ?? 0}</dd></div>
      </dl>
    </section>
  {/if}

  {#if contextMenu}
    <div
      class="sidebar-context-menu"
      style:left={`${contextMenu.x}px`}
      style:top={`${contextMenu.y}px`}
      role="menu"
      tabindex="-1"
    >
      <button type="button" role="menuitem" on:click={revealContextPath}>
        <FolderOpen size={15} />
        <span>在文件管理器中显示</span>
      </button>
    </div>
  {/if}
</aside>
