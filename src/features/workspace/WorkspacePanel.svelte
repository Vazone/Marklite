<script lang="ts">
  import { tick } from 'svelte';
  import { translator } from '../../lib/i18n';
  import { isSameResource, type ResourceRef } from '../../lib/platform/resources';
  import { FileClock, FolderClosed, FolderOpen, FolderPlus, LocateFixed, RefreshCw, X } from 'lucide-svelte';
  import type { WorkspaceStore } from './store';
  import WorkspaceTree from './WorkspaceTree.svelte';
  export let store: WorkspaceStore;
  export let activePath: string | null = null;
  export let onRevealPath: (path: string) => void = () => undefined;
  let selectedPath: string | null = null;
  let notice = '';
  $: if (!$store.root) selectedPath = null;
  $: watchIssue = $store.listenerIssue ?? [...$store.nodes.values()].find(node => node.watchIssue)?.watchIssue;

  function location(resource: ResourceRef) {
    return resource.kind === 'desktopDirectory' ? resource.path : $translator('workspace.androidLocation');
  }
  function resourceKey(resource: ResourceRef) {
    return resource.kind === 'desktopDirectory' || resource.kind === 'desktopFile'
      ? `desktop:${resource.path}` : `android:${resource.uri}`;
  }

  async function locate() {
    notice = '';
    const resource = $store.root?.resource;
    if (resource?.kind !== 'desktopDirectory' || !activePath) return;
    const root = resource.path.replace(/\\/g, '/').replace(/\/$/, '') + '/';
    const file = activePath.replace(/\\/g, '/');
    const relative = file.startsWith(root) ? file.slice(root.length) : null;
    if (relative && await store.reveal(relative)) { selectedPath = null; await tick(); selectedPath = relative; }
    else notice = $translator('workspace.outside');
  }
</script>

<section class="workspace-panel" aria-label={$translator('workspace.files')}>
  <header class="workspace-header">
    <h2>{$translator('workspace.files')}</h2>
    <button class="choose-folder" disabled={$store.supported !== true} on:click={() => store.choose()}>
      <FolderPlus size={16} aria-hidden="true" /> <span>{$translator('workspace.choose')}</span>
    </button>
  </header>

  {#if $store.root}
    <div class="current-folder">
      <div class="folder-identity" title={location($store.root.resource)}>
        <FolderClosed size={18} aria-hidden="true" />
        <span><strong>{$store.root.name}</strong><small>{location($store.root.resource)}</small></span>
      </div>
      <div class="folder-actions">
        <button title={$translator('workspace.refresh')} aria-label={$translator('workspace.refresh')} on:click={() => store.refresh()}><RefreshCw size={16} /></button>
        <button disabled={!activePath} title={$translator('workspace.locate')} aria-label={$translator('workspace.locate')} on:click={locate}><LocateFixed size={16} /></button>
        <button title={$translator('workspace.close')} aria-label={$translator('workspace.close')} on:click={() => store.clear()}><X size={16} /></button>
      </div>
    </div>
  {:else if !$store.loading}
    <div class="folder-empty">
      <FolderOpen size={28} aria-hidden="true" />
      <strong>{$translator('workspace.noFolder')}</strong>
      <span>{$translator('workspace.noFolderHint')}</span>
    </div>
  {/if}

  {#if $store.supported === false}<p class="folder-notice">{$translator('workspace.unsupported')}</p>{/if}
  {#if $store.loading}<p class="folder-notice" role="status">{$translator('workspace.loading')}</p>{/if}
  {#if $store.issue || $store.persistenceIssue}
    <div class="folder-notice error" role="status">
      <span>{($store.issue ?? $store.persistenceIssue)?.message}</span>
      {#if !$store.root && $store.savedRoot}<button on:click={() => store.retry()}>{$translator('workspace.retry')}</button>{/if}
    </div>
  {/if}
  {#if notice}<p class="folder-notice" role="status">{notice}</p>{/if}
  {#if watchIssue}<p class="folder-notice" role="status">{$translator('workspace.watchUnavailable')} {watchIssue.message}</p>{/if}

  {#if $store.root}
    {#if $store.nodes.get('')?.loaded && !$store.nodes.get('')?.entries.length}<p class="folder-notice">{$translator('workspace.empty')}</p>{/if}
    <div class="tree-container">
      <WorkspaceTree {store} {selectedPath} {onRevealPath} label={$translator('workspace.files')} moreLabel={$translator('workspace.more')}
        retryLabel={$translator('workspace.retry')} loadingLabel={$translator('workspace.loading')} />
    </div>
  {/if}

  {#if $store.recent.length}
    <div class="recent-folders">
      <div class="section-label"><FileClock size={15} aria-hidden="true" /><h3>{$translator('workspace.recent')}</h3><span>{$store.recent.length}</span></div>
      <div class="recent-items" role="list">
        {#each $store.recent as entry (resourceKey(entry.resource))}
          {@const current = isSameResource($store.root?.resource, entry.resource)}
          <div class="recent-item" class:current role="listitem">
            <button class="recent-open" aria-current={current ? 'page' : undefined} title={location(entry.resource)}
              on:click={() => store.openRecent(entry.resource)}>
              <FolderClosed size={16} aria-hidden="true" />
              <span><strong>{entry.name}</strong><small>{location(entry.resource)}</small></span>
            </button>
            <button class="recent-forget" title={$translator('workspace.forget')} aria-label={`${$translator('workspace.forget')}: ${entry.name}`}
              on:click={() => store.forgetRecent(entry.resource)}><X size={15} /></button>
          </div>
        {/each}
      </div>
    </div>
  {/if}

  <label class="restore-setting">
    <input type="checkbox" checked={$store.restoreOnStart} on:change={event => store.setRestoreOnStart(event.currentTarget.checked)} />
    <span>{$translator('workspace.restoreOnStart')}</span>
  </label>
</section>

<style>
  .workspace-panel { min-height: 0; min-width: 0; display: flex; flex-direction: column; overflow: hidden; }
  .workspace-header { display: flex; align-items: center; justify-content: space-between; gap: 8px; padding: 12px; }
  .workspace-header h2 { margin: 0; font-size: .78rem; font-weight: 700; letter-spacing: .04em; color: var(--text-secondary); }
  button { font: inherit; cursor: pointer; }
  .choose-folder { display: inline-flex; align-items: center; gap: 5px; border: 1px solid var(--border-color); border-radius: var(--radius-md); background: var(--panel-bg); color: inherit; padding: 5px 7px; font-size: .75rem; white-space: nowrap; }
  .choose-folder:hover, .folder-actions button:hover, .recent-forget:hover { background: var(--bg-secondary); }
  button:disabled { opacity: .5; cursor: default; }
  .current-folder { margin: 0 8px 8px; border: 1px solid var(--border-color); border-radius: var(--radius-md); background: color-mix(in srgb, var(--accent-color) 5%, var(--panel-bg)); }
  .folder-identity { display: flex; align-items: center; gap: 9px; min-width: 0; padding: 10px; color: var(--accent-color); }
  .folder-identity > span, .recent-open > span { display: flex; flex-direction: column; min-width: 0; flex: 1; gap: 2px; }
  .folder-identity strong, .recent-open strong { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: .83rem; color: var(--text-primary); }
  .folder-identity small, .recent-open small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: .7rem; color: var(--text-secondary); }
  .folder-actions { display: flex; gap: 2px; padding: 0 6px 6px; justify-content: flex-end; }
  .folder-actions button, .recent-forget { display: grid; place-items: center; width: 29px; height: 28px; border: 0; border-radius: 5px; color: var(--text-secondary); background: transparent; }
  .folder-notice { display: flex; align-items: center; gap: 8px; margin: 4px 10px 8px; padding: 8px; border-radius: 6px; background: var(--bg-secondary); font-size: .75rem; overflow-wrap: anywhere; }
  .folder-notice.error { border-left: 3px solid var(--accent-color); }
  .folder-notice button { flex-shrink: 0; border: 1px solid var(--border-color); background: var(--panel-bg); border-radius: 5px; padding: 5px; }
  .folder-empty { display: flex; flex: 1; flex-direction: column; align-items: center; justify-content: center; gap: 8px; min-height: 120px; padding: 20px; text-align: center; color: var(--text-secondary); }
  .folder-empty strong { font-size: .88rem; color: var(--text-primary); }
  .folder-empty span { font-size: .76rem; line-height: 1.4; }
  .tree-container { flex: 1; min-height: 80px; border-top: 1px solid var(--border-color); }
  .recent-folders { flex: 0 1 auto; min-height: 0; border-top: 1px solid var(--border-color); }
  .section-label { display: flex; align-items: center; gap: 6px; padding: 10px 12px 5px; color: var(--text-secondary); }
  .section-label h3 { flex: 1; margin: 0; font-size: .73rem; font-weight: 700; }
  .section-label span { font-size: .7rem; }
  .recent-items { max-height: min(30vh, 230px); overflow: auto; padding: 0 6px 8px; }
  .recent-item { display: flex; align-items: center; border-radius: 6px; }
  .recent-item:hover, .recent-item.current { background: color-mix(in srgb, var(--accent-color) 10%, transparent); }
  .recent-open { display: flex; flex: 1; align-items: center; gap: 8px; min-width: 0; border: 0; background: transparent; color: var(--text-secondary); text-align: left; padding: 7px 6px; }
  .recent-item.current .recent-open { color: var(--accent-color); }
  .recent-forget { flex-shrink: 0; margin-right: 2px; }
  .restore-setting { display: flex; align-items: center; gap: 7px; padding: 9px 12px; border-top: 1px solid var(--border-color); color: var(--text-secondary); font-size: .73rem; cursor: pointer; }
  .restore-setting input { accent-color: var(--accent-color); }
  @media (pointer: coarse) {
    .choose-folder, .recent-open, .recent-forget, .folder-actions button { min-height: 44px; }
    .recent-items { max-height: 32vh; }
  }
</style>
