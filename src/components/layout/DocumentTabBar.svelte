<script lang="ts">
  import { onMount, tick } from 'svelte';
  import type { EditorTab } from '../../app/stores/documentStore';

  export let tabs: EditorTab[] = [];
  export let activeTabId: string | null = null;
  export let onActivate: (tabId: string) => void = () => {};
  export let onClose: (tabId: string) => void = () => {};
  export let onCloseOthers: (tabId: string) => void = () => {};
  export let onCloseRight: (tabId: string) => void = () => {};

  type ContextMenuState = {
    tabId: string;
    x: number;
    y: number;
  };

  const menuWidth = 210;
  const menuHeight = 116;
  const viewportPadding = 8;
  let contextMenu: ContextMenuState | null = null;
  let menuElement: HTMLDivElement | null = null;
  let triggerElement: HTMLButtonElement | null = null;

  $: menuTabIndex = contextMenu ? tabs.findIndex((tab) => tab.id === contextMenu?.tabId) : -1;
  $: canCloseOthers = menuTabIndex >= 0 && tabs.length > 1;
  $: canCloseRight = menuTabIndex >= 0 && menuTabIndex < tabs.length - 1;
  $: if (contextMenu && menuTabIndex < 0) closeContextMenu(false);

  onMount(() => {
    const onPointerDown = (event: PointerEvent) => {
      if (!contextMenu || menuElement?.contains(event.target as Node)) return;
      closeContextMenu(false);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Escape' || !contextMenu) return;
      event.preventDefault();
      closeContextMenu(true);
    };
    const onViewportChange = () => closeContextMenu(false);

    window.addEventListener('pointerdown', onPointerDown, true);
    window.addEventListener('keydown', onKeyDown);
    window.addEventListener('resize', onViewportChange);
    window.addEventListener('scroll', onViewportChange, true);
    return () => {
      window.removeEventListener('pointerdown', onPointerDown, true);
      window.removeEventListener('keydown', onKeyDown);
      window.removeEventListener('resize', onViewportChange);
      window.removeEventListener('scroll', onViewportChange, true);
    };
  });

  function clampPosition(x: number, y: number) {
    return {
      x: Math.max(viewportPadding, Math.min(x, window.innerWidth - menuWidth - viewportPadding)),
      y: Math.max(viewportPadding, Math.min(y, window.innerHeight - menuHeight - viewportPadding))
    };
  }

  async function openContextMenu(tabId: string, x: number, y: number, trigger: HTMLButtonElement) {
    const position = clampPosition(x, y);
    triggerElement = trigger;
    contextMenu = { tabId, ...position };
    await tick();
    menuElement?.querySelector<HTMLButtonElement>('button:not(:disabled)')?.focus();
  }

  function handleMouseContextMenu(event: MouseEvent, tabId: string) {
    event.preventDefault();
    event.stopPropagation();
    const trigger = event.currentTarget as HTMLElement;
    const tabButton = trigger.querySelector<HTMLButtonElement>('.tab-main');
    if (tabButton) void openContextMenu(tabId, event.clientX, event.clientY, tabButton);
  }

  function handleTabKeyDown(event: KeyboardEvent, tabId: string) {
    if (event.key === 'ContextMenu' || (event.shiftKey && event.key === 'F10')) {
      event.preventDefault();
      const trigger = event.currentTarget as HTMLButtonElement;
      const rect = trigger.getBoundingClientRect();
      void openContextMenu(tabId, rect.left + Math.min(rect.width, menuWidth) / 2, rect.bottom, trigger);
      return;
    }
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      onActivate(tabId);
    }
  }

  function closeContextMenu(restoreFocus: boolean) {
    const previousTrigger = triggerElement;
    contextMenu = null;
    menuElement = null;
    triggerElement = null;
    if (restoreFocus) void tick().then(() => previousTrigger?.focus());
  }

  function runMenuAction(action: (tabId: string) => void) {
    const tabId = contextMenu?.tabId;
    if (!tabId) return;
    closeContextMenu(false);
    action(tabId);
  }
</script>

<div class="tabbar" role="tablist" aria-label="打开的文档">
  {#each tabs as tab}
    <div
      class="document-tab"
      class:active={tab.id === activeTabId}
      data-tab-id={tab.id}
      data-load-state={tab.loadState}
      role="group"
      aria-label={tab.title}
      on:contextmenu={(event) => handleMouseContextMenu(event, tab.id)}
    >
      <button
        type="button"
        class="tab-main"
        role="tab"
        aria-selected={tab.id === activeTabId}
        tabindex={tab.id === activeTabId ? 0 : -1}
        title={tab.path ?? tab.title}
        on:click={() => onActivate(tab.id)}
        on:keydown={(event) => handleTabKeyDown(event, tab.id)}
      >
        <span class:dirty-dot={tab.isDirty}></span>
        <span class="tab-title" class:long-title={tab.title.length > 18}>
          <span>{tab.title}</span>
        </span>
      </button>
      <button
        type="button"
        class="tab-close"
        aria-label={`关闭 ${tab.title}`}
        title="关闭标签页"
        on:click|stopPropagation={() => onClose(tab.id)}
      >
        ×
      </button>
    </div>
  {/each}
</div>

{#if contextMenu}
  <div
    bind:this={menuElement}
    class="tab-context-menu"
    role="menu"
    aria-label="标签页操作"
    style={`left: ${contextMenu.x}px; top: ${contextMenu.y}px;`}
  >
    <button type="button" role="menuitem" on:click={() => runMenuAction(onClose)}>关闭标签页</button>
    <button type="button" role="menuitem" disabled={!canCloseOthers} on:click={() => runMenuAction(onCloseOthers)}>
      关闭其他标签页
    </button>
    <button type="button" role="menuitem" disabled={!canCloseRight} on:click={() => runMenuAction(onCloseRight)}>
      关闭右侧标签页
    </button>
  </div>
{/if}
