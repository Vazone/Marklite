<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import {
    adjustSidebarWidth,
    clampSidebarDragWidth,
    sidebarMaximum,
    sidebarWidthFromClientX,
    MIN_SIDEBAR_WIDTH,
    SIDEBAR_COLLAPSE_THRESHOLD
  } from '../../lib/sidebarResize';
  import { createPointerResizeLifecycle, type PointerResizeLifecycle } from '../../lib/pointerResizeLifecycle';
  import { translator } from '../../lib/i18n';

  export let width = 280;
  export let onWidthChange: (width: number) => void;
  export let onCommit: (rawWidth: number, visibleWidth: number, workspaceWidth: number) => void;
  export let onCollapse: () => void;

  let separator: HTMLElement;
  let workspace: HTMLElement | null = null;
  let workspaceWidth = 0;
  let dragging = false;
  let collapseIntent = false;
  let startWidth = width;
  let dragLifecycle: PointerResizeLifecycle;
  let resizeObserver: ResizeObserver | null = null;
  let mediaQuery: MediaQueryList | null = null;

  $: maximum = sidebarMaximum(workspaceWidth);

  onMount(() => {
    workspace = separator.parentElement;
    mediaQuery = window.matchMedia?.('(max-width: 920px)') ?? null;
    measureWorkspace();
    window.addEventListener('resize', measureWorkspace);
    window.addEventListener('blur', cancelActiveDrag);
    mediaQuery?.addEventListener('change', handleViewportChange);
    if (workspace && typeof ResizeObserver !== 'undefined') {
      resizeObserver = new ResizeObserver(measureWorkspace);
      resizeObserver.observe(workspace);
    }
    dragLifecycle = createPointerResizeLifecycle({
      element: separator,
      bodyClass: 'sidebar-resize-dragging',
      onSample: previewPointer,
      onStart: () => {
        dragging = true;
      },
      onCommit: () => {
        dragging = false;
      },
      onCancel: restoreStartWidth
    });
  });

  onDestroy(() => {
    dragLifecycle?.dispose();
    resizeObserver?.disconnect();
    window.removeEventListener('resize', measureWorkspace);
    window.removeEventListener('blur', cancelActiveDrag);
    mediaQuery?.removeEventListener('change', handleViewportChange);
  });

  function isNarrowViewport(): boolean {
    return mediaQuery?.matches ?? window.innerWidth <= 920;
  }

  function handleViewportChange(): void {
    if (isNarrowViewport()) cancelActiveDrag();
    else measureWorkspace();
  }

  function measureWorkspace(): void {
    if (!workspace || isNarrowViewport()) return;
    const measured = workspace.getBoundingClientRect().width;
    if (measured <= 0) return;
    workspaceWidth = measured;
    const clamped = Math.min(
      sidebarMaximum(measured),
      Math.max(MIN_SIDEBAR_WIDTH, width)
    );
    if (!dragLifecycle?.active && Math.abs(clamped - width) > 0.5) {
      onCommit(clamped, clamped, workspaceWidth);
    }
  }

  function pointerWidth(clientX: number): { raw: number; visible: number } {
    const rect = workspace?.getBoundingClientRect();
    if (!rect) return { raw: width, visible: width };
    const raw = sidebarWidthFromClientX(clientX, rect.left);
    return { raw, visible: clampSidebarDragWidth(raw, rect.width) };
  }

  function handlePointerDown(event: PointerEvent): void {
    if (event.button !== 0 || event.isPrimary === false || isNarrowViewport()) return;
    event.preventDefault();
    startWidth = width;
    collapseIntent = false;
    dragLifecycle.start(event);
    dragLifecycle.schedule(event.clientX);
  }

  function handlePointerMove(event: PointerEvent): void {
    if (!dragLifecycle.active || event.pointerId !== dragLifecycle.pointerId) return;
    event.preventDefault();
    dragLifecycle.schedule(event.clientX);
  }

  function handlePointerUp(event: PointerEvent): void {
    if (!dragLifecycle.active || event.pointerId !== dragLifecycle.pointerId) return;
    event.preventDefault();
    const { raw, visible } = pointerWidth(event.clientX);
    onWidthChange(visible);
    onCommit(raw, visible, workspaceWidth);
    collapseIntent = false;
    dragLifecycle.commit(event);
  }

  function handlePointerCancel(event: PointerEvent): void {
    dragLifecycle.cancel(event);
  }

  function handleLostPointerCapture(event: PointerEvent): void {
    dragLifecycle.handleLostPointerCapture(event);
  }

  function cancelActiveDrag(): void {
    dragLifecycle?.cancelActive();
  }

  function restoreStartWidth(): void {
    dragging = false;
    onWidthChange(startWidth);
    collapseIntent = false;
  }

  function previewPointer(clientX: number): void {
    const { raw, visible } = pointerWidth(clientX);
    collapseIntent = raw <= SIDEBAR_COLLAPSE_THRESHOLD;
    onWidthChange(visible);
  }

  function handleKeyDown(event: KeyboardEvent): void {
    if (event.key === 'Home') {
      event.preventDefault();
      onCollapse();
      return;
    }
    if (event.key === 'End') {
      event.preventDefault();
      onCommit(maximum, maximum, workspaceWidth);
      return;
    }
    if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return;
    event.preventDefault();
    const next = adjustSidebarWidth(
      width,
      event.key === 'ArrowLeft' ? -1 : 1,
      workspaceWidth,
      event.shiftKey
    );
    onWidthChange(next);
    onCommit(next, next, workspaceWidth);
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex a11y_no_noninteractive_element_interactions -->
<div
  bind:this={separator}
  class="sidebar-resize-separator"
  class:dragging
  class:collapse-intent={collapseIntent}
  role="separator"
  aria-label={$translator('sidebar.resize')}
  aria-orientation="vertical"
  aria-valuemin={MIN_SIDEBAR_WIDTH}
  aria-valuemax={Math.round(maximum)}
  aria-valuenow={Math.round(width)}
  tabindex="0"
  on:pointerdown={handlePointerDown}
  on:pointermove={handlePointerMove}
  on:pointerup={handlePointerUp}
  on:pointercancel={handlePointerCancel}
  on:lostpointercapture={handleLostPointerCapture}
  on:keydown={handleKeyDown}
>
  <span aria-hidden="true"></span>
  {#if collapseIntent}
    <em aria-hidden="true">{$translator('sidebar.resizeCollapse')}</em>
  {/if}
</div>

<style>
  .sidebar-resize-separator {
    position: relative;
    z-index: 5;
    display: grid;
    width: 8px;
    min-width: 8px;
    height: 100%;
    place-items: center;
    cursor: col-resize;
    touch-action: none;
  }

  .sidebar-resize-separator > span {
    width: 1px;
    height: 100%;
    background: var(--border-color);
    transition: width 120ms ease, background 120ms ease;
  }

  .sidebar-resize-separator:hover > span,
  .sidebar-resize-separator:focus-visible > span,
  .sidebar-resize-separator.dragging > span {
    width: 3px;
    background: var(--accent-color);
  }

  .sidebar-resize-separator.collapse-intent > span {
    background: var(--warning-color);
  }

  .sidebar-resize-separator:focus-visible {
    outline: 2px solid color-mix(in srgb, var(--accent-color) 55%, transparent);
    outline-offset: -2px;
  }

  em {
    position: absolute;
    top: 12px;
    left: 8px;
    padding: 5px 8px;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-md);
    background: var(--panel-bg);
    color: var(--text-primary);
    box-shadow: var(--shadow-soft);
    font-size: 0.75rem;
    font-style: normal;
    white-space: nowrap;
    pointer-events: none;
  }

  :global(body.sidebar-resize-dragging) {
    cursor: col-resize !important;
    user-select: none !important;
  }

  @media (max-width: 920px) {
    .sidebar-resize-separator {
      display: none;
      pointer-events: none;
    }
  }
</style>
