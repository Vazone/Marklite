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
  import { createPointerResizeLifecycle, type PointerResizeLifecycle } from '../../lib/pointerResizeLifecycle';
  import { translator } from '../../lib/i18n';

  export let ratio = 0.5;
  export let onRatioChange: (ratio: number) => void;
  export let onCommit: (rawRatio: number, visibleRatio: number) => void;
  export let onCollapse: (mode: 'edit' | 'preview') => void;

  let separator: HTMLElement;
  let container: HTMLElement | null = null;
  let containerWidth = 0;
  let dragging = false;
  let startRatio = ratio;
  let dropIntent: SplitDropMode = 'split';
  let previewOffsetPx = 0;
  let dragLifecycle: PointerResizeLifecycle;
  let resizeObserver: ResizeObserver | null = null;
  let mediaQuery: MediaQueryList | null = null;

  $: bounds = splitRatioBounds(containerWidth);
  $: ariaValue = Math.round(ratio * 100);
  $: dropMessage =
    dropIntent === 'preview'
      ? $translator('split.releasePreview')
      : dropIntent === 'edit'
        ? $translator('split.releaseEdit')
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
    dragLifecycle = createPointerResizeLifecycle({
      element: separator,
      bodyClass: 'split-pane-dragging',
      onSample: previewPointer,
      onStart: () => {
        dragging = true;
      },
      onCommit: () => {
        dragging = false;
      },
      onCancel: restoreStartRatio
    });
  });

  onDestroy(() => {
    dragLifecycle?.dispose();
    resizeObserver?.disconnect();
    mediaQuery?.removeEventListener('change', measureContainer);
    window.removeEventListener('resize', measureContainer);
    window.removeEventListener('blur', cancelActiveDrag);
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
    startRatio = ratio;
    dropIntent = 'split';
    dragLifecycle.start(event);
    dragLifecycle.schedule(event.clientX);
  }

  function handlePointerMove(event: PointerEvent) {
    if (!dragLifecycle.active || event.pointerId !== dragLifecycle.pointerId) return;
    event.preventDefault();
    dragLifecycle.schedule(event.clientX);
  }

  function handlePointerUp(event: PointerEvent) {
    if (!dragLifecycle.active || event.pointerId !== dragLifecycle.pointerId) return;
    event.preventDefault();
    const { raw, visible } = pointerRatio(event.clientX);
    onCommit(raw, visible);
    previewOffsetPx = 0;
    dropIntent = 'split';
    dragLifecycle.commit(event);
  }

  function handlePointerCancel(event: PointerEvent) {
    dragLifecycle.cancel(event);
  }

  function cancelActiveDrag() {
    dragLifecycle?.cancelActive();
  }

  function handleLostPointerCapture(event: PointerEvent) {
    dragLifecycle.handleLostPointerCapture(event);
  }

  function restoreStartRatio() {
    dragging = false;
    previewOffsetPx = 0;
    dropIntent = 'split';
  }

  function previewPointer(clientX: number) {
    const { raw, visible } = pointerRatio(clientX);
    dropIntent = splitDropMode(raw);
    previewOffsetPx = (visible - startRatio) * containerWidth;
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
  style={`--split-preview-offset: ${previewOffsetPx}px`}
  role="separator"
  aria-label={$translator('split.resize')}
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
    transform: translateX(var(--split-preview-offset));
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
