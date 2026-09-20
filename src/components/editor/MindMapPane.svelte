<script lang="ts">
  import { onDestroy } from 'svelte';
  import type { OutlineItem } from '../../lib/tauriApi';
  import {
    buildMindMap,
    collectCollapsibleNodeIds,
    createMindMapViewportIndex,
    layoutMindMap,
    MIND_MAP_MAX_HEADINGS,
    projectMindMapViewport,
    type MindMapNodeSize
  } from '../../lib/mindMap';
  import { translator } from '../../lib/i18n';

  export let documentTitle: string;
  export let outline: OutlineItem[];
  export let onJumpToLine: (line: number) => void;

  let collapsedIds = new Set<string>();
  let measuredNodeSizes = new Map<string, MindMapNodeSize>();
  let nodeSizeObserver: ResizeObserver | null = null;
  const observedNodeIds = new WeakMap<Element, string>();
  let panViewport: HTMLElement;
  let panPointerId: number | null = null;
  let panClientX = 0;
  let panClientY = 0;
  let projectionFrame: number | null = null;
  let viewport = { left: 0, top: 0, width: 1200, height: 800 };
  const MEASURED_NODE_LIMIT = 2_000;

  $: exceedsLayoutLimit = outline.length > MIND_MAP_MAX_HEADINGS;
  $: tree = buildMindMap(documentTitle, exceedsLayoutLimit ? [] : outline);
  $: pruneMeasuredNodeSizes(tree);
  $: layout = layoutMindMap(tree, collapsedIds, measuredNodeSizes);
  $: viewportIndex = createMindMapViewportIndex(layout);
  $: projection = projectMindMapViewport(viewportIndex, viewport);

  onDestroy(() => {
    nodeSizeObserver?.disconnect();
    if (projectionFrame !== null) cancelAnimationFrame(projectionFrame);
    panPointerId = null;
  });

  function getNodeSizeObserver(): ResizeObserver | null {
    if (nodeSizeObserver) return nodeSizeObserver;
    if (typeof ResizeObserver === 'undefined') return null;
    nodeSizeObserver = new ResizeObserver((entries) => {
      let next: Map<string, MindMapNodeSize> | null = null;
      for (const entry of entries) {
        if (entry.target === panViewport) {
          scheduleViewportProjection();
          continue;
        }
        const id = observedNodeIds.get(entry.target);
        const width = (entry.target as HTMLElement).offsetWidth;
        const height = (entry.target as HTMLElement).offsetHeight;
        const previous = id ? measuredNodeSizes.get(id) : undefined;
        if (
          !id ||
          width <= 0 ||
          height <= 0 ||
          (Math.abs((previous?.width ?? 0) - width) < 0.5 &&
            Math.abs((previous?.height ?? 0) - height) < 0.5)
        ) {
          continue;
        }
        next ??= new Map(measuredNodeSizes);
        next.set(id, { width, height });
      }
      if (next) measuredNodeSizes = next;
    });
    return nodeSizeObserver;
  }

  function observeNode(element: HTMLElement, nodeId: string) {
    if (outline.length > MEASURED_NODE_LIMIT) return {};
    observedNodeIds.set(element, nodeId);
    getNodeSizeObserver()?.observe(element);
    return {
      update(nextNodeId: string) {
        observedNodeIds.set(element, nextNodeId);
      },
      destroy() {
        nodeSizeObserver?.unobserve(element);
        observedNodeIds.delete(element);
      }
    };
  }

  function observeViewport(element: HTMLElement) {
    getNodeSizeObserver()?.observe(element);
    scheduleViewportProjection();
    return {
      destroy() {
        nodeSizeObserver?.unobserve(element);
      }
    };
  }

  function scheduleViewportProjection() {
    if (!panViewport || projectionFrame !== null) return;
    projectionFrame = requestAnimationFrame(() => {
      projectionFrame = null;
      viewport = {
        left: panViewport.scrollLeft,
        top: panViewport.scrollTop,
        width: panViewport.clientWidth || 1200,
        height: panViewport.clientHeight || 800
      };
    });
  }

  function pruneMeasuredNodeSizes(root: ReturnType<typeof buildMindMap>) {
    if (!measuredNodeSizes.size) return;
    const validIds = new Set<string>();
    const stack = [root];
    while (stack.length) {
      const node = stack.pop()!;
      validIds.add(node.id);
      for (let index = node.children.length - 1; index >= 0; index -= 1) {
        stack.push(node.children[index]);
      }
    }
    if ([...measuredNodeSizes.keys()].every((id) => validIds.has(id))) return;
    measuredNodeSizes = new Map(
      [...measuredNodeSizes.entries()].filter(([id]) => validIds.has(id))
    );
  }

  function startCanvasPan(event: PointerEvent) {
    const target = event.target as Element | null;
    if (
      event.button !== 0 ||
      !target ||
      target.closest('button, a, input, select, textarea, [role="button"]')
    ) {
      return;
    }
    panPointerId = event.pointerId;
    panClientX = event.clientX;
    panClientY = event.clientY;
    panViewport.setPointerCapture?.(event.pointerId);
    event.preventDefault();
  }

  function moveCanvasPan(event: PointerEvent) {
    if (event.pointerId !== panPointerId) return;
    panViewport.scrollLeft -= event.clientX - panClientX;
    panViewport.scrollTop -= event.clientY - panClientY;
    panClientX = event.clientX;
    panClientY = event.clientY;
    scheduleViewportProjection();
    event.preventDefault();
  }

  function finishCanvasPan(event: PointerEvent, releaseCapture: boolean) {
    if (event.pointerId !== panPointerId) return;
    panPointerId = null;
    if (releaseCapture && panViewport.hasPointerCapture?.(event.pointerId)) {
      panViewport.releasePointerCapture(event.pointerId);
    }
  }

  function toggleNode(id: string) {
    const next = new Set(collapsedIds);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    collapsedIds = next;
  }

  function collapseAll() {
    collapsedIds = new Set(collectCollapsibleNodeIds(tree));
  }

  function expandAll() {
    collapsedIds = new Set();
  }
</script>

<section class="mind-map" aria-label={$translator('mindMap.ariaLabel')}>
  <div class="mind-map-actions" aria-label={$translator('mindMap.actions')}>
    <span>{$translator('mindMap.nodeCount', { count: outline.length })}</span>
    <button type="button" on:click={expandAll}>{$translator('mindMap.expandAll')}</button>
    <button type="button" on:click={collapseAll}>{$translator('mindMap.collapseAll')}</button>
  </div>

  {#if exceedsLayoutLimit}
    <div class="mind-map-empty" role="alert">
      <strong>{$translator('mindMap.tooLargeTitle')}</strong>
      <span>{$translator('mindMap.tooLargeDescription', { count: outline.length, limit: MIND_MAP_MAX_HEADINGS })}</span>
    </div>
  {:else if outline.length === 0}
    <div class="mind-map-empty">
      <strong>{$translator('mindMap.emptyTitle')}</strong>
      <span>{$translator('mindMap.emptyDescription')}</span>
    </div>
  {:else}
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div
      bind:this={panViewport}
      use:observeViewport
      class="mind-map-viewport"
      class:dragging={panPointerId !== null}
      role="region"
      tabindex="0"
      aria-label={$translator('mindMap.canvas')}
      on:pointerdown={startCanvasPan}
      on:pointermove={moveCanvasPan}
      on:pointerup={(event) => finishCanvasPan(event, true)}
      on:pointercancel={(event) => finishCanvasPan(event, true)}
      on:lostpointercapture={(event) => finishCanvasPan(event, false)}
      on:scroll={scheduleViewportProjection}
    >
      <div
        class="mind-map-canvas"
        style:width={`${layout.width}px`}
        style:height={`${layout.height}px`}
        data-total-layout-nodes={layout.nodes.length}
        data-rendered-layout-nodes={projection.nodes.length}
      >
        <svg
          class="mind-map-connectors"
          width={layout.width}
          height={layout.height}
          aria-hidden="true"
        >
          {#each projection.connectors as connector (connector.id)}
            <path d={connector.path} class={`mind-map-connector level-${connector.level}`} />
          {/each}
        </svg>

        {#each projection.nodes as node (node.id)}
          <article
            class={`mind-map-node level-${node.level}`}
            class:root-node={node.level === 0}
            use:observeNode={node.id}
            style:left={`${node.x}px`}
            style:top={`${node.y}px`}
            data-layout-width={node.width}
            data-layout-height={node.height}
          >
            <button
              type="button"
              class="mind-map-node-label"
              title={node.line ? `${node.label} (${$translator('common.line', { line: node.line })})` : node.label}
              disabled={node.line === null}
              on:click={() => node.line && onJumpToLine(node.line)}
            >
              <span>{node.label}</span>
              {#if node.line}
                <small>{$translator('common.line', { line: node.line })}</small>
              {/if}
            </button>
            {#if node.hasChildren}
              <button
                type="button"
                class="mind-map-node-toggle"
                aria-label={$translator('mindMap.toggle', { action: node.collapsed ? $translator('mindMap.expand') : $translator('mindMap.collapse'), label: node.label })}
                aria-expanded={!node.collapsed}
                on:click={() => toggleNode(node.id)}
              >{node.collapsed ? '+' : '−'}</button>
            {/if}
          </article>
        {/each}
      </div>
    </div>
  {/if}
</section>
