<script lang="ts">
  import type { EditorTab } from '../app/stores/documentStore';
  import Sidebar from '../components/layout/Sidebar.svelte';
  import type { RecentFileDto } from '../lib/tauriApi';

  export let recentFiles: RecentFileDto[] = [];
  export let firstTab: EditorTab;
  export let secondTab: EditorTab;
  export let unsavedTab: EditorTab;

  let activeTab = firstTab;
  let visibleRecentFiles = recentFiles;

  function removeRecent(path: string) {
    visibleRecentFiles = visibleRecentFiles.filter((file) => file.path !== path);
  }
</script>

<button type="button" data-testid="first-tab" on:click={() => (activeTab = firstTab)}>First</button>
<button type="button" data-testid="second-tab" on:click={() => (activeTab = secondTab)}>Second</button>
<button type="button" data-testid="unsaved-tab" on:click={() => (activeTab = unsavedTab)}>Unsaved</button>
<Sidebar
  activeSidebarTab="recent"
  recentFiles={visibleRecentFiles}
  tab={activeTab}
  onRemoveRecent={removeRecent}
  onTabChange={() => undefined}
  onOpenRecent={() => undefined}
  onRevealRecent={() => undefined}
  onJumpToLine={() => undefined}
  onCollapse={() => undefined}
/>
