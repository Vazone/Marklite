<script lang="ts">
  import { onMount } from 'svelte';
  import { CircleAlert, FileText, FolderClosed, FolderOpen } from 'lucide-svelte';
  import { clampContextMenuPosition, type ContextMenuPosition } from '../../lib/contextMenuPosition';
  import { translator } from '../../lib/i18n';
  import type { WorkspaceStore } from './store';
  import { treeRows, treeWindow, type TreeRow } from './tree';

  export let store: WorkspaceStore;
  export let label: string;
  export let moreLabel: string;
  export let retryLabel: string;
  export let loadingLabel: string;
  export let selectedPath: string | null = null;
  export let onRevealPath: (path: string) => void = () => undefined;
  let viewport: HTMLDivElement;
  let height = 400, scrollTop = 0, focusKey = '', prefix = '', typedAt = 0;
  let rowHeight = 34;
  let contextMenu: (ContextMenuPosition & { path: string }) | null = null;
  $: rows = treeRows($store);
  $: range = treeWindow(rows.length, scrollTop, height, rowHeight);
  $: focusIndex = Math.max(0, rows.findIndex(row => row.key === focusKey));
  // Keep the active descendant mounted even when the wheel moves it outside the viewport.
  $: indices = [...new Set([...Array.from({ length: range.end - range.start }, (_, i) => range.start + i),
    ...(rows.length ? [focusIndex] : [])])].sort((a, b) => a - b);
  $: if (selectedPath) focusPath(selectedPath, rows);

  onMount(() => {
    rowHeight = window.matchMedia?.('(pointer: coarse)').matches ? 44 : 34;
    const observer = new ResizeObserver(entries => { height = entries[0].contentRect.height; });
    observer.observe(viewport);
    const close = () => { contextMenu = null; };
    const closeOnEscape = (event: KeyboardEvent) => { if (event.key === 'Escape') close(); };
    window.addEventListener('click', close);
    window.addEventListener('keydown', closeOnEscape);
    return () => {
      observer.disconnect();
      window.removeEventListener('click', close);
      window.removeEventListener('keydown', closeOnEscape);
    };
  });

  function openContextMenu(event: MouseEvent, row: TreeRow) {
    const resource = row.entry?.resource;
    if (resource?.kind !== 'desktopFile' && resource?.kind !== 'desktopDirectory') return;
    event.preventDefault();
    event.stopPropagation();
    contextMenu = { ...clampContextMenuPosition(event.clientX, event.clientY, 220, 48,
      window.innerWidth, window.innerHeight), path: resource.path };
  }

  function focusPath(path: string, values: TreeRow[]) {
    const index = values.findIndex(row => row.action === 'entry' && row.path === path);
    if (index >= 0 && values[index].key !== focusKey) move(index);
  }
  function move(index: number) {
    const bounded = Math.max(0, Math.min(rows.length - 1, index));
    if (!rows[bounded]) return;
    focusKey = rows[bounded].key;
    const top = bounded * rowHeight;
    if (viewport && (top < viewport.scrollTop || top + rowHeight > viewport.scrollTop + height)) {
      viewport.scrollTop = top < viewport.scrollTop ? top : top + rowHeight - height;
      scrollTop = viewport.scrollTop;
    }
  }
  function activate(row: TreeRow) {
    if (row.action === 'more') void store.load(row.path, true);
    else if (row.action === 'retry') void store.refresh(row.path);
    else if (row.entry?.kind === 'directory') {
      if ($store.nodes.has(row.path)) void store.collapse(row.path);
      else void store.expand(row.path);
    } else if (row.entry?.kind === 'markdown') void store.open(row.path);
  }
  function keydown(event: KeyboardEvent) {
    if (event.altKey || event.ctrlKey || event.metaKey || !rows.length) return;
    const row = rows[focusIndex];
    switch (event.key) {
      case 'ArrowDown': move(focusIndex + 1); break;
      case 'ArrowUp': move(focusIndex - 1); break;
      case 'Home': move(0); break;
      case 'End': move(rows.length - 1); break;
      case 'Enter': case ' ': activate(row); break;
      case 'ArrowRight':
        if (row.entry?.kind === 'directory') {
          if (!$store.nodes.has(row.path)) void store.expand(row.path);
          else if (rows[focusIndex + 1]?.parent === row.path) move(focusIndex + 1);
        }
        break;
      case 'ArrowLeft':
        if (row.entry?.kind === 'directory' && $store.nodes.has(row.path)) void store.collapse(row.path);
        else { const index = rows.findIndex(item => item.action === 'entry' && item.path === row.parent); if (index >= 0) move(index); }
        break;
      default:
        if (event.key.length !== 1) return;
        prefix = Date.now() - typedAt > 700 ? event.key.toLocaleLowerCase() : prefix + event.key.toLocaleLowerCase();
        typedAt = Date.now();
        for (let step = 1; step <= rows.length; step++) {
          const index = (focusIndex + step) % rows.length;
          if (rows[index].entry?.name.toLocaleLowerCase().startsWith(prefix)) { move(index); break; }
        }
    }
    event.preventDefault();
    event.stopPropagation();
  }
  function text(row: TreeRow) {
    return row.entry?.name ?? (row.action === 'more' ? moreLabel : row.action === 'retry'
      ? `${retryLabel}: ${$store.nodes.get(row.path)?.issue?.message ?? ''}` : loadingLabel);
  }
</script>

<div class="workspace-tree" bind:this={viewport} role="tree" tabindex="0" aria-label={label}
  aria-activedescendant={rows.length ? `workspace-row-${focusIndex}` : undefined}
  on:keydown={keydown} on:scroll={() => { scrollTop = viewport.scrollTop; contextMenu = null; }}>
  <div class="tree-space" role="presentation" style:height={`${range.totalHeight}px`}>
    {#each indices as index (rows[index].key)}
      {@const row = rows[index]}
      <div id={`workspace-row-${index}`} role="treeitem" tabindex="-1" class="tree-row"
        class:focused={index === focusIndex} class:current={row.path === selectedPath && row.action === 'entry'}
        aria-level={row.depth} aria-posinset={row.position} aria-setsize={row.count}
        aria-expanded={row.entry?.kind === 'directory' ? $store.nodes.has(row.path) : undefined}
        aria-selected={row.path === selectedPath && row.action === 'entry'}
        aria-disabled={row.entry?.kind === 'unavailable' || row.action === 'loading'}
        title={row.entry?.issue?.message ?? text(row)}
        style:top={`${index * rowHeight}px`} style:height={`${rowHeight}px`} style:padding-left={`${8 + (row.depth - 1) * 16}px`}
        on:click={() => { focusKey = row.key; viewport.focus(); activate(row); }} on:keydown={keydown}
        on:contextmenu={(event) => openContextMenu(event, row)}>
        <span class="tree-marker" aria-hidden="true">{row.entry?.kind === 'directory' ? ($store.nodes.has(row.path) ? '▾' : '▸') : row.entry ? '·' : ''}</span>
        {#if row.entry?.kind === 'directory'}
          {#if $store.nodes.has(row.path)}<FolderOpen size={15} aria-hidden="true" />{:else}<FolderClosed size={15} aria-hidden="true" />{/if}
        {:else if row.entry?.kind === 'markdown'}<FileText size={15} aria-hidden="true" />
        {:else if row.entry?.kind === 'unavailable'}<CircleAlert size={15} aria-hidden="true" />{/if}
        <span class="tree-name">{text(row)}</span>
      </div>
    {/each}
  </div>
  {#if contextMenu}
    <div class="sidebar-context-menu" role="menu" style:left={`${contextMenu.x}px`} style:top={`${contextMenu.y}px`}
      style:width={`${contextMenu.width}px`} style:max-height={`${contextMenu.maxHeight}px`}>
      <button type="button" role="menuitem" on:click={() => {
        if (contextMenu) onRevealPath(contextMenu.path);
        contextMenu = null;
      }}><FolderOpen size={15} aria-hidden="true" /><span>{$translator('sidebar.reveal')}</span></button>
    </div>
  {/if}
</div>

<style>
  .workspace-tree { min-height: 0; height: 100%; overflow: auto; outline: none; position: relative; }
  .tree-space { position: relative; min-width: 0; }
  .tree-row { position: absolute; box-sizing: border-box; width: 100%; display: flex; align-items: center; gap: 6px; padding-right: 8px; cursor: pointer; font-size: 12px; }
  .tree-row :global(svg) { flex-shrink: 0; color: var(--text-secondary); }
  .tree-marker { width: 12px; flex-shrink: 0; color: var(--text-secondary); }
  .tree-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tree-row:hover, .current { background: color-mix(in srgb, var(--accent-color) 10%, transparent); }
  .current .tree-name { color: var(--accent-color); font-weight: 600; }
  .workspace-tree:focus .focused { outline: 1px solid var(--accent, #648bde); outline-offset: -1px; }
  [aria-disabled="true"] { opacity: .6; cursor: default; }
</style>
