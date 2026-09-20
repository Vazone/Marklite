<script lang="ts">
  import { onMount, tick } from 'svelte';
  import type { DocumentTabDescriptor } from '../../app/stores/documentStore';
  import { clampContextMenuPosition, type ContextMenuPosition } from '../../lib/contextMenuPosition';
  import { translator } from '../../lib/i18n';

  export let tabs: DocumentTabDescriptor[] = [];
  export let activeTabId: string | null = null;
  export let onActivate: (tabId: string) => void;
  export let onClose: (tabId: string) => void | Promise<void>;
  export let onCloseOthers: (tabId: string) => void | Promise<void>;
  export let onCloseRight: (tabId: string) => void | Promise<void>;

  type ContextMenuState = ContextMenuPosition & {
    tabId: string;
  };

  const menuWidth = 210;
  const menuHeight = 116;
  let tabbarElement: HTMLDivElement | null = null;
  let contextMenu: ContextMenuState | null = null;
  let menuElement: HTMLDivElement | null = null;
  let triggerElement: HTMLButtonElement | null = null;
  let revealRequest = 0;

  $: menuTabIndex = contextMenu ? tabs.findIndex((tab) => tab.id === contextMenu?.tabId) : -1;
  $: canCloseOthers = menuTabIndex >= 0 && tabs.length > 1;
  $: canCloseRight = menuTabIndex >= 0 && menuTabIndex < tabs.length - 1;
  $: if (contextMenu && menuTabIndex < 0) closeContextMenu(false);
  $: void revealActiveTab(activeTabId, tabs.length);

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

  async function openContextMenu(tabId: string, x: number, y: number, trigger: HTMLButtonElement) {
    const position = clampContextMenuPosition(
      x,
      y,
      menuWidth,
      menuHeight,
      window.innerWidth,
      window.innerHeight
    );
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
    if (!event.altKey && !event.ctrlKey && !event.metaKey && !event.shiftKey) {
      const current = tabs.findIndex((tab) => tab.id === tabId);
      const destination = event.key === 'Home' ? 0 : event.key === 'End' ? tabs.length - 1
        : event.key === 'ArrowRight' ? (current + 1) % tabs.length
        : event.key === 'ArrowLeft' ? (current - 1 + tabs.length) % tabs.length : -1;
      if (current >= 0 && destination >= 0) {
        event.preventDefault();
        const next = tabs[destination];
        onActivate(next.id);
        void tick().then(() => focusTab(next.id));
        return;
      }
      if (event.key === 'Delete') {
        event.preventDefault();
        void closeWithFocus(tabId);
        return;
      }
    }
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

  function focusTab(tabId: string) {
    const button = [...(tabbarElement?.querySelectorAll<HTMLButtonElement>('.tab-main') ?? [])]
      .find((element) => element.closest<HTMLElement>('.document-tab')?.dataset.tabId === tabId);
    button?.focus();
  }

  async function closeWithFocus(tabId: string) {
    const index = tabs.findIndex((tab) => tab.id === tabId);
    await onClose(tabId);
    await tick();
    if (tabs.some((tab) => tab.id === tabId)) {
      focusTab(tabId);
    } else if (tabs.length) {
      focusTab(activeTabId && tabs.some((tab) => tab.id === activeTabId)
        ? activeTabId : tabs[Math.min(index, tabs.length - 1)].id);
    }
  }

  function closeContextMenu(restoreFocus: boolean) {
    const previousTrigger = triggerElement;
    contextMenu = null;
    menuElement = null;
    triggerElement = null;
    if (restoreFocus) void tick().then(() => previousTrigger?.focus());
  }

  async function runMenuAction(action: (tabId: string) => void | Promise<void>) {
    const tabId = contextMenu?.tabId;
    if (!tabId) return;
    closeContextMenu(false);
    if (action === onClose) {
      await closeWithFocus(tabId);
      return;
    }
    await action(tabId);
    await tick();
    focusTab(tabs.some((tab) => tab.id === tabId) ? tabId : activeTabId ?? '');
  }

  async function revealActiveTab(tabId: string | null, tabCount: number) {
    const request = ++revealRequest;
    if (!tabId || tabCount === 0) return;

    await tick();
    if (request !== revealRequest || tabId !== activeTabId) return;

    const activeElement = [...(tabbarElement?.querySelectorAll<HTMLElement>('.document-tab') ?? [])].find(
      (element) => element.dataset.tabId === tabId
    );
    activeElement?.scrollIntoView?.({ block: 'nearest', inline: 'nearest' });
  }

  function handleWheel(event: WheelEvent) {
    const container = event.currentTarget as HTMLDivElement;
    const maximumScrollLeft = Math.max(0, container.scrollWidth - container.clientWidth);
    if (maximumScrollLeft === 0) return;

    const rawDelta = Math.abs(event.deltaX) > Math.abs(event.deltaY) ? event.deltaX : event.deltaY;
    if (!Number.isFinite(rawDelta) || rawDelta === 0) return;

    const deltaScale = event.deltaMode === 1 ? 32 : event.deltaMode === 2 ? container.clientWidth : 1;
    const currentScrollLeft = Math.max(0, Math.min(container.scrollLeft, maximumScrollLeft));
    const nextScrollLeft = Math.max(
      0,
      Math.min(currentScrollLeft + rawDelta * deltaScale, maximumScrollLeft)
    );
    if (nextScrollLeft === currentScrollLeft) return;

    container.scrollLeft = nextScrollLeft;
    event.preventDefault();
  }
</script>

<div
  bind:this={tabbarElement}
  class="tabbar"
  role="tablist"
  aria-label={$translator('tabs.ariaLabel')}
  on:wheel|nonpassive={handleWheel}
>
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
        <span class="tab-title">
          <span>{tab.title}</span>
        </span>
      </button>
      <button
        type="button"
        class="tab-close"
        aria-label={$translator('tabs.close', { title: tab.title })}
        title={$translator('tabs.closeTab')}
        on:click|stopPropagation={() => void closeWithFocus(tab.id)}
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
    aria-label={$translator('tabs.menu')}
    style:left={`${contextMenu.x}px`}
    style:top={`${contextMenu.y}px`}
    style:width={`${contextMenu.width}px`}
    style:max-height={`${contextMenu.maxHeight}px`}
    style:overflow-y="auto"
  >
    <button type="button" role="menuitem" on:click={() => void runMenuAction(onClose)}>{$translator('tabs.closeTab')}</button>
    <button type="button" role="menuitem" disabled={!canCloseOthers} on:click={() => void runMenuAction(onCloseOthers)}>
      {$translator('tabs.closeOthers')}
    </button>
    <button type="button" role="menuitem" disabled={!canCloseRight} on:click={() => void runMenuAction(onCloseRight)}>
      {$translator('tabs.closeRight')}
    </button>
  </div>
{/if}
