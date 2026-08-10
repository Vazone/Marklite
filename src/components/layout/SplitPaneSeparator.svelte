<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import {
    adjustSplitRatio,
    clampVisibleSplitRatio,
    splitDropMode,
    splitRatioBounds,
    splitRatioFromClientX,
    type SplitDropMode
  } from '../../lib/splitPane';

  export let ratio = 0.5;
  export let onRatioChange: (ratio: number) => void = () => {};
  export let onCommit: (rawRatio: number, visibleRatio: number) => void = () => {};
  export let onCollapse: (mode: 'edit' | 'preview') => void = () => {};

  let separator: HTMLElement;
  let container: HTMLElement | null = null;
  let containerWidth = 0;
  let dragging = false;
  let pointerId: number | null = null;
  let startRatio = ratio;
  let dropIntent: SplitDropMode = 'split';
  let frame: number | null = null;
  let pendingClientX: number | null = null;
  let resizeObserver: ResizeObserver | null = null;
  let mediaQuery: MediaQueryList | null = null;

  $: bounds = splitRatioBounds(containerWidth);
  $: ariaValue = Math.round(ratio * 100);
  $: dropMessage =
    dropIntent === 'preview'
      ? '释放后切换为预览'
      : dropIntent === 'edit'
        ? '释放后切换为编辑'
        : '';

  onMount(() => {
    container = separator.parentElement;
    mediaQuery = window.matchMedia?.('(max-width: 920px)') ?? null;
    measureContainer();
    mediaQuery?.addEventListener('change', measureContainer);
    window.addEventListener('resize', measureContainer);
    window.addEventListener('blur', cancelActiveDrag);
    if (container && typeof ResizeObserver !== 'undefined') {
      resizeObserver = new ResizeObserver(measureContainer);
      resizeObserver.observe(container);
    }
  });

  onDestroy(() => {
    cancelFrame();
    resizeObserver?.disconnect();
    mediaQuery?.removeEventListener('change', measureContainer);
    window.removeEventListener('resize', measureContainer);
    window.removeEventListener('blur', cancelActiveDrag);
    endGlobalDragState();
  });

  function isNarrowViewport() {
    return mediaQuery?.matches ?? window.innerWidth <= 920;
  }

  function measureContainer() {
    if (!container || isNarrowViewport()) return;
    const width = container.getBoundingClientRect().width;
    if (width <= 0) return;
    containerWidth = width;
    const clamped = clampVisibleSplitRatio(ratio, width);
    if (Math.abs(clamped - ratio) > 0.0001) onRatioChange(clamped);
  }

  function pointerRatio(clientX: number) {
    if (!container) return { raw: ratio, visible: ratio };
    const rect = container.getBoundingClientRect();
    const raw = splitRatioFromClientX(clientX, rect.left, rect.width);
    return { raw, visible: clampVisibleSplitRatio(raw, rect.width) };
  }

  function handlePointerDown(event: PointerEvent) {
    if (event.button !== 0 || event.isPrimary === false || isNarrowViewport()) return;
    event.preventDefault();
    dragging = true;
    pointerId = event.pointerId;
    startRatio = ratio;
    dropIntent = 'split';
    separator.setPointerCapture?.(event.pointerId);
    document.body.classList.add('split-pane-dragging');
    schedulePointer(event.clientX);
  }

  function handlePointerMove(event: PointerEvent) {
    if (!dragging || event.pointerId !== pointerId) return;
    event.preventDefault();
    schedulePointer(event.clientX);
  }

  function handlePointerUp(event: PointerEvent) {
    if (!dragging || event.pointerId !== pointerId) return;
    event.preventDefault();
    cancelFrame();
    const { raw, visible } = pointerRatio(event.clientX);
    onRatioChange(visible);
    onCommit(raw, visible);
    dropIntent = 'split';
    endPointerCapture(event.pointerId);
    endGlobalDragState();
  }

  function handlePointerCancel(event: PointerEvent) {
    if (!dragging || event.pointerId !== pointerId) return;
    cancelFrame();
    onRatioChange(startRatio);
    dropIntent = 'split';
    endPointerCapture(event.pointerId);
    endGlobalDragState();
  }

  function cancelActiveDrag() {
    if (!dragging) return;
    cancelFrame();
    onRatioChange(startRatio);
    dropIntent = 'split';
    if (pointerId !== null) endPointerCapture(pointerId);
    endGlobalDragState();
  }

  function handleLostPointerCapture(event: PointerEvent) {
    if (!dragging || event.pointerId !== pointerId) return;
    cancelFrame();
    onRatioChange(startRatio);
    dropIntent = 'split';
    endGlobalDragState();
  }

  function schedulePointer(clientX: number) {
    pendingClientX = clientX;
    if (frame !== null) return;
    frame = requestAnimationFrame(() => {
      frame = null;
      if (pendingClientX === null || !dragging) return;
      const { raw, visible } = pointerRatio(pendingClientX);
      pendingClientX = null;
      dropIntent = splitDropMode(raw);
      onRatioChange(visible);
    });
  }

  function cancelFrame() {
    if (frame !== null) cancelAnimationFrame(frame);
    frame = null;
    pendingClientX = null;
  }

  function endPointerCapture(id: number) {
    if (separator.hasPointerCapture?.(id)) separator.releasePointerCapture(id);
  }

  function endGlobalDragState() {
    dragging = false;
    pointerId = null;
    document.body.classList.remove('split-pane-dragging');
  }

  function handleKeyDown(event: KeyboardEvent) {
    if (event.key === 'Home') {
      event.preventDefault();
      onCollapse('preview');
      return;
    }
    if (event.key === 'End') {
      event.preventDefault();
      onCollapse('edit');
      return;
    }
    if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return;
    event.preventDefault();
    const width = container?.getBoundingClientRect().width ?? containerWidth;
    const next = adjustSplitRatio(
      ratio,
      event.key === 'ArrowLeft' ? -1 : 1,
      width,
      event.shiftKey
    );
    onRatioChange(next);
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex a11y_no_noninteractive_element_interactions -->
<div
  bind:this={separator}
  class="split-separator"
  class:dragging
  class:collapse-preview={dropIntent === 'preview'}
  class:collapse-edit={dropIntent === 'edit'}
  data-drop-intent={dropIntent}
  role="separator"
  aria-label="调整编辑器和预览宽度"
  aria-orientation="vertical"
  aria-valuemin={Math.round(bounds.min * 100)}
  aria-valuemax={Math.round(bounds.max * 100)}
  aria-valuenow={ariaValue}
  tabindex="0"
  on:pointerdown={handlePointerDown}
  on:pointermove={handlePointerMove}
  on:pointerup={handlePointerUp}
  on:pointercancel={handlePointerCancel}
  on:lostpointercapture={handleLostPointerCapture}
  on:keydown={handleKeyDown}
>
  <span class="split-separator-line" aria-hidden="true"></span>
  {#if dropMessage}
    <span class="split-drop-hint" aria-hidden="true">{dropMessage}</span>
  {/if}
</div>

<style>
  .split-separator {
    position: relative;
    z-index: 4;
    display: grid;
    width: 8px;
    min-width: 8px;
    height: 100%;
    padding: 0;
    place-items: center;
    border: 0;
    background: transparent;
    cursor: col-resize;
    touch-action: none;
  }

  .split-separator-line {
    width: 1px;
    height: 100%;
    background: var(--border-color);
    transition: width 120ms ease, background 120ms ease;
  }

  .split-separator:hover .split-separator-line,
  .split-separator:focus-visible .split-separator-line,
  .split-separator.dragging .split-separator-line {
    width: 3px;
    background: var(--accent-color);
  }

  .split-separator:focus-visible {
    outline: 2px solid color-mix(in srgb, var(--accent-color) 55%, transparent);
    outline-offset: -2px;
  }

  .split-separator.collapse-preview .split-separator-line,
  .split-separator.collapse-edit .split-separator-line {
    background: var(--accent-color);
  }

  .split-drop-hint {
    position: absolute;
    top: 12px;
    left: 50%;
    z-index: 5;
    padding: 5px 8px;
    transform: translateX(-50%);
    border: 1px solid var(--border-color);
    border-radius: var(--radius-md);
    background: var(--panel-bg);
    color: var(--text-primary);
    box-shadow: var(--shadow-soft);
    font-size: 0.75rem;
    white-space: nowrap;
    pointer-events: none;
  }

  :global(body.split-pane-dragging) {
    cursor: col-resize !important;
    user-select: none !important;
  }

  @media (max-width: 920px) {
    .split-separator {
      display: none;
      pointer-events: none;
    }
  }
</style>
