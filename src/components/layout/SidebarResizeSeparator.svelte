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

  export let width = 280;
  export let onWidthChange: (width: number) => void = () => {};
  export let onCommit: (rawWidth: number, visibleWidth: number, workspaceWidth: number) => void = () => {};
  export let onCollapse: () => void = () => {};

  let separator: HTMLElement;
  let workspace: HTMLElement | null = null;
  let workspaceWidth = 0;
  let dragging = false;
  let collapseIntent = false;
  let pointerId: number | null = null;
  let startWidth = width;
  let frame: number | null = null;
  let pendingClientX: number | null = null;
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
  });

  onDestroy(() => {
    cancelActiveDrag();
    cancelFrame();
    resizeObserver?.disconnect();
    window.removeEventListener('resize', measureWorkspace);
    window.removeEventListener('blur', cancelActiveDrag);
    mediaQuery?.removeEventListener('change', handleViewportChange);
    endGlobalDragState();
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
    if (!dragging && Math.abs(clamped - width) > 0.5) {
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
    dragging = true;
    pointerId = event.pointerId;
    startWidth = width;
    collapseIntent = false;
    separator.setPointerCapture?.(event.pointerId);
    document.body.classList.add('sidebar-resize-dragging');
    schedulePointer(event.clientX);
  }

  function handlePointerMove(event: PointerEvent): void {
    if (!dragging || event.pointerId !== pointerId) return;
    event.preventDefault();
    schedulePointer(event.clientX);
  }

  function handlePointerUp(event: PointerEvent): void {
    if (!dragging || event.pointerId !== pointerId) return;
    event.preventDefault();
    cancelFrame();
    const { raw, visible } = pointerWidth(event.clientX);
    onWidthChange(visible);
    onCommit(raw, visible, workspaceWidth);
    endGlobalDragState();
    endPointerCapture(event.pointerId);
  }

  function handlePointerCancel(event: PointerEvent): void {
    if (!dragging || event.pointerId !== pointerId) return;
    cancelActiveDrag();
  }

  function handleLostPointerCapture(event: PointerEvent): void {
    if (!dragging || event.pointerId !== pointerId) return;
    cancelActiveDrag();
  }

  function cancelActiveDrag(): void {
    if (!dragging) return;
    cancelFrame();
    onWidthChange(startWidth);
    const capturedPointerId = pointerId;
    endGlobalDragState();
    if (capturedPointerId !== null) endPointerCapture(capturedPointerId);
  }

  function schedulePointer(clientX: number): void {
    pendingClientX = clientX;
    if (frame !== null) return;
    frame = requestAnimationFrame(() => {
      frame = null;
      if (!dragging || pendingClientX === null) return;
      const { raw, visible } = pointerWidth(pendingClientX);
      pendingClientX = null;
      collapseIntent = raw <= SIDEBAR_COLLAPSE_THRESHOLD;
      onWidthChange(visible);
    });
  }

  function cancelFrame(): void {
    if (frame !== null) cancelAnimationFrame(frame);
    frame = null;
    pendingClientX = null;
  }

  function endPointerCapture(id: number): void {
    if (separator.hasPointerCapture?.(id)) separator.releasePointerCapture(id);
  }

  function endGlobalDragState(): void {
    dragging = false;
    collapseIntent = false;
    pointerId = null;
    document.body.classList.remove('sidebar-resize-dragging');
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
  aria-label="调整最近文件侧栏宽度"
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
    <em aria-hidden="true">释放后收起侧栏</em>
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
