<script lang="ts">
  import { onDestroy, tick } from 'svelte';
  import type { AppSettings, DiagramDiagnostic, DiagramSource, DiagramTheme, OutlineItem, RenderedDiagram, SanitizedMarkdownHtml, SourceBlock, VirtualPreviewIndex, VirtualPreviewWindow } from '../../lib/tauriApi';
  import { api, openEmailLink, openExternalLink, toAppError } from '../../lib/tauriApi';
  import { executeMarkdownTarget } from '../../lib/markdownNavigation';
  import { uiActions } from '../../app/stores/uiStore';
  import type { EditorScrollPosition } from '../../app/stores/documentStore';
  import { buildSourceBlockScrollMap, findSourceBlockTarget, findVisibleSourceBlockLine, SOURCE_BLOCK_ATTRIBUTE, tagSourceBlockElements, type SourceBlockScrollMap } from '../../lib/sourceBlockScroll';
  import { createSplitScrollController, type SplitScrollAnchor } from '../../lib/splitScrollController';
  import { createLatestTaskQueue } from '../../lib/latestTaskQueue';
  import {
    measurePreviewChunks,
    PREVIEW_CHUNK_MIN_HTML_LENGTH,
    rescalePreviewChunks,
    wrapPreviewChunks,
    type PreviewChunks
  } from '../../lib/previewChunks';
  import { createLazyComponent } from '../../lib/lazyComponent';
  import { localizeError, t, translator } from '../../lib/i18n';
  import { observeMediaQuery } from '../../lib/mediaQuery';
  import { previewDiagramRuntime } from '../../lib/previewDiagramRuntime';
  import { previewHeightPrefix, previewSegmentAtLine, previewSegmentAtPixel, previewSegmentAtSource, previewWindowAround } from '../../lib/virtualPreviewLayout';

  export let html: SanitizedMarkdownHtml;
  export let tabId = '';
  export let contentRevision = -1;
  export let renderedRevision = -1;
  export let sourceBlocks: SourceBlock[] = [];
  export let virtualPreview: VirtualPreviewIndex | null = null;
  export let diagrams: DiagramSource[] = [];
  export let diagramDiagnostics: DiagramDiagnostic[] = [];
  export let settings: AppSettings;
  export let documentPath: string | null = null;
  export let documentTitle: string;
  export let outline: OutlineItem[];
  export let onOpenDocument: (path: string, fragment: string | null) => Promise<boolean>;
  export let onJumpToLine: (line: number) => void;
  export let onSyncEditorLine: (line: number) => void = () => undefined;
  export let onCopySource: () => Promise<void> = async () => undefined;

  type PreviewMode = 'article' | 'mindMap';
  const mindMapLoader = createLazyComponent(() => import('./MindMapPane.svelte'));
  let MindMapPane = mindMapLoader.current();
  let previewMode: PreviewMode = 'article';

  let previewHost: HTMLElement;
  let resourceRevision = 0;
  let scheduledHtml: string | null = null;
  let scheduledDocumentPath: string | null = null;
  let scheduledAllowLocalImages: boolean | null = null;
  let scheduledTabId: string | null = null;
  let scheduledRenderedRevision = -1;
  let scheduledDiagrams: DiagramSource[] | null = null;
  let scheduledDiagramDiagnostics: DiagramDiagnostic[] | null = null;
  let scheduledDiagramTheme: DiagramTheme | null = null;
  let scheduledDiagramTypography = '';
  let committedHtml = '';
  let renderedNodes: Node[] = [];
  let committedWithoutImages = false;
  let activeResourceJobId: string | null = null;
  let displayedObjectUrls: string[] = [];
  let previewMaxScroll = 0;
  let previewScrollRangeDirty = true;
  let lastPreviewTypography = '';
  let previewChunks: PreviewChunks | null = null;
  let activeVirtualPreview: VirtualPreviewIndex | null = null;
  let virtualHeights: number[] = [];
  let virtualPrefix: number[] = [0];
  let virtualWindowStart = -1;
  let virtualWindowEnd = -1;
  let virtualWindowBlocks: SourceBlock[] = [];
  let virtualWindowBlockStart = 0;
  let desiredVirtualWindow = '';
  let virtualScrollFrame = 0;
  let userPreviewScrollUntil = 0;
  let previewPointerActive = false;
  let pendingVirtualAnchor: SplitScrollAnchor | null = null;
  let sourceScrollMap: SourceBlockScrollMap | null = null;
  let committedIdentity = '';
  let committedSettled = false;
  let pendingFragment: { fragment: string; identity: string } | null = null;
  let chunkResizeObserver: ResizeObserver | null = null;
  let chunkScaleFrame = 0;
  let chunkSeenFrame = 0;
  let scrollCorrectionFrame = 0;
  const scrollController = createSplitScrollController(applyEditorScroll);
  const systemThemeMedia = typeof window === 'undefined' || !window.matchMedia
    ? null
    : window.matchMedia('(prefers-color-scheme: dark)');
  let systemDark = systemThemeMedia?.matches ?? false;
  const stopObservingSystemTheme = systemThemeMedia
    ? observeMediaQuery(systemThemeMedia, (matches) => { systemDark = matches; })
    : () => undefined;
  let diagramTheme: DiagramTheme = 'light';
  let diagramTypography = '';

  $: scrollController.setActive(tabId, contentRevision);
  $: diagramTheme = settings.theme === 'system' ? (systemDark ? 'dark' : 'light') : settings.theme;
  $: diagramTypography = `${settings.previewFontFamily}\u0000${settings.previewFontSize}\u0000${settings.lineHeight}`;

  type PreviewPreparation = {
    html: string;
    documentPath: string | null;
    allowLocalImages: boolean;
    diagrams: DiagramSource[];
    diagramDiagnostics: DiagramDiagnostic[];
    diagramTheme: DiagramTheme;
    fontFamily: string;
    fontSize: number;
    lineHeight: number;
    revision: number;
    jobId: string;
  };

  const preparationQueue = createLatestTaskQueue<PreviewPreparation, void>(async (request) => {
    try {
      await preparePreview(
        request.html,
        request.documentPath,
        request.allowLocalImages,
        request.diagrams,
        request.diagramDiagnostics,
        request.diagramTheme,
        request.fontFamily,
        request.fontSize,
        request.lineHeight,
        request.revision,
        request.jobId
      );
    } finally {
      if (activeResourceJobId === request.jobId) activeResourceJobId = null;
    }
  });

  const virtualWindowQueue = createLatestTaskQueue<{ sessionId: string; start: number; end: number }, VirtualPreviewWindow>(
    ({ sessionId, start, end }) => api.renderMarkdownWindow(sessionId, start, end)
  );

  $: if (
    previewMode === 'article' &&
    !virtualPreview &&
    previewHost &&
    (html !== scheduledHtml ||
      documentPath !== scheduledDocumentPath ||
      settings.allowLocalImages !== scheduledAllowLocalImages ||
      tabId !== scheduledTabId ||
      renderedRevision !== scheduledRenderedRevision ||
      diagrams !== scheduledDiagrams ||
      diagramDiagnostics !== scheduledDiagramDiagnostics ||
      diagramTheme !== scheduledDiagramTheme ||
      diagramTypography !== scheduledDiagramTypography)
  ) {
    scheduledHtml = html;
    scheduledDocumentPath = documentPath;
    scheduledAllowLocalImages = settings.allowLocalImages;
    scheduledTabId = tabId;
    scheduledRenderedRevision = renderedRevision;
    scheduledDiagrams = diagrams;
    scheduledDiagramDiagnostics = diagramDiagnostics;
    scheduledDiagramTheme = diagramTheme;
    scheduledDiagramTypography = diagramTypography;
    schedulePreviewPreparation(html, documentPath, settings.allowLocalImages, diagrams, diagramDiagnostics, diagramTheme);
  }

  $: if (
    previewMode === 'article' && previewHost && virtualPreview &&
    renderedRevision === contentRevision && virtualPreview !== activeVirtualPreview
  ) {
    activateVirtualPreview(virtualPreview);
  }

  $: if (previewMode === 'article' && previewHost && !virtualPreview && activeVirtualPreview) {
    deactivateVirtualPreview();
  }

  $: if (previewHost) {
    const typography = `${settings.previewFontFamily}\u0000${settings.previewFontSize}\u0000${settings.lineHeight}`;
    if (typography !== lastPreviewTypography) {
      lastPreviewTypography = typography;
      previewScrollRangeDirty = true;
      if (previewChunks) {
        void tick().then(() => {
          if (previewHost && previewChunks) {
            rescalePreviewChunks(previewHost, previewChunks, true);
            refreshPreviewScrollRange();
            if (committedSettled) scrollController.layoutReady();
          }
        });
      }
    }
  }

  onDestroy(() => {
    resourceRevision += 1;
    cancelActiveResourceJob();
    preparationQueue.dispose();
    virtualWindowQueue.dispose();
    if (activeVirtualPreview) void api.releaseMarkdownPreview(activeVirtualPreview.sessionId);
    previewDiagramRuntime.release();
    stopObservingSystemTheme();
    chunkResizeObserver?.disconnect();
    cancelAnimationFrame(chunkScaleFrame);
    cancelAnimationFrame(chunkSeenFrame);
    cancelAnimationFrame(scrollCorrectionFrame);
    cancelAnimationFrame(virtualScrollFrame);
    scrollController.dispose();
    revokeObjectUrls(displayedObjectUrls);
    displayedObjectUrls = [];
  });

  export function syncToEditorScroll(position: EditorScrollPosition | undefined, identity?: { tabId: string; contentRevision: number }) {
    if (!position) return;
    userPreviewScrollUntil = 0;
    previewPointerActive = false;
    scrollController.update({
      tabId: identity?.tabId ?? tabId,
      contentRevision: identity?.contentRevision ?? contentRevision,
      position
    });
  }

  function applyEditorScroll(anchor: SplitScrollAnchor) {
    if (previewMode !== 'article' || !settings.syncScroll || !previewHost) return;
    const offset = anchor.position.offsetUtf16;
    if (activeVirtualPreview && renderedRevision === contentRevision && Number.isSafeInteger(offset)) {
      const blocks = virtualWindowBlocks;
      if (!blocks.length || offset! < blocks[0].startUtf16 || offset! >= blocks[blocks.length - 1].endUtf16) {
        const index = previewSegmentAtSource(activeVirtualPreview.segments, offset!);
        pendingVirtualAnchor = anchor;
        previewHost.scrollTop = virtualPrefix[index] ?? 0;
        requestVirtualWindow(index, true);
        return;
      }
    }
    if (sourceScrollMap && renderedRevision === contentRevision && Number.isSafeInteger(offset)) {
      const target = findSourceBlockTarget(sourceScrollMap, offset!);
      if (alignSourceTarget(target.element, target.fraction, anchor)) return;
    }
    if (previewScrollRangeDirty) refreshPreviewScrollRange();
    previewHost.scrollTop = previewMaxScroll * Math.min(1, Math.max(0, anchor.position.ratio));
  }

  function alignSourceTarget(element: HTMLElement, fraction: number, anchor: SplitScrollAnchor, retry = true): boolean {
    if (!previewHost?.contains(element)) return false;
    const hostRect = previewHost.getBoundingClientRect();
    const targetRect = element.getBoundingClientRect();
    if (targetRect.height === 0 && previewChunks) {
      const leaf = element.closest<HTMLElement>('.preview-chunk');
      if (!leaf) return false;
      if (!retry) {
        element.scrollIntoView({ block: 'start' });
        return true;
      }
      previewHost.scrollTop += leaf.getBoundingClientRect().top - hostRect.top;
      cancelAnimationFrame(scrollCorrectionFrame);
      scrollCorrectionFrame = requestAnimationFrame(() => {
        scrollCorrectionFrame = 0;
        if (anchor.tabId === tabId && anchor.contentRevision === contentRevision) {
          alignSourceTarget(element, fraction, anchor, false);
        }
      });
      return true;
    }
    previewHost.scrollTop += targetRect.top - hostRect.top + targetRect.height * fraction - 2;
    return true;
  }

  function activateVirtualPreview(next: VirtualPreviewIndex) {
    if (activeVirtualPreview) void api.releaseMarkdownPreview(activeVirtualPreview.sessionId);
    resourceRevision += 1;
    cancelActiveResourceJob();
    previewDiagramRuntime.cancel();
    revokeObjectUrls(displayedObjectUrls);
    displayedObjectUrls = [];
    clearPreviewChunks();
    for (const node of renderedNodes) node.parentNode?.removeChild(node);
    renderedNodes = [];
    committedHtml = '';
    committedSettled = false;
    virtualWindowBlocks = [];
    virtualWindowStart = -1;
    virtualWindowEnd = -1;
    desiredVirtualWindow = '';
    activeVirtualPreview = next;
    virtualHeights = next.segments.map((segment) => segment.estimatedHeight);
    virtualPrefix = previewHeightPrefix(virtualHeights);
    if (previewHost) previewHost.scrollTop = 0;
    requestVirtualWindow(0, true);
  }

  function deactivateVirtualPreview() {
    if (activeVirtualPreview) void api.releaseMarkdownPreview(activeVirtualPreview.sessionId);
    activeVirtualPreview = null;
    desiredVirtualWindow = '';
    virtualWindowStart = -1;
    virtualWindowEnd = -1;
    virtualWindowBlocks = [];
    virtualHeights = [];
    virtualPrefix = [0];
    pendingVirtualAnchor = null;
    cancelAnimationFrame(virtualScrollFrame);
    virtualScrollFrame = 0;
  }

  function requestVirtualWindow(index: number, force = false) {
    const current = activeVirtualPreview;
    if (!current || !previewHost || !current.segments.length) return;
    if (!force && virtualWindowStart >= 0 && index >= virtualWindowStart + 1 && index < virtualWindowEnd - 1) return;
    const { start, end } = previewWindowAround(current.segments, virtualPrefix, index, previewHost.clientHeight);
    const key = `${current.sessionId}:${start}:${end}`;
    if (key === desiredVirtualWindow) return;
    desiredVirtualWindow = key;
    void virtualWindowQueue.submit({ sessionId: current.sessionId, start, end }).then((result) => {
      if (result.status !== 'completed' || desiredVirtualWindow !== key ||
        activeVirtualPreview?.sessionId !== current.sessionId) return;
      const window = result.value;
      if (window.sessionId !== current.sessionId || window.start !== start || window.end !== end) return;
      const visibleIndex = previewSegmentAtPixel(virtualPrefix, previewHost.scrollTop);
      if (visibleIndex < start || visibleIndex >= end) {
        desiredVirtualWindow = '';
        requestVirtualWindow(visibleIndex, true);
        return;
      }
      virtualWindowStart = start;
      virtualWindowEnd = end;
      virtualWindowBlockStart = 0;
      virtualWindowBlocks = window.segments.flatMap((segment) => segment.sourceBlocks);
      const top = Math.round(virtualPrefix[start]);
      const bottom = Math.round(virtualPrefix[virtualPrefix.length - 1] - virtualPrefix[end]);
      let blockOffset = 0;
      const windowHtml = window.segments.map((segment) => {
        const offset = blockOffset;
        blockOffset += segment.sourceBlocks.length;
        const html = segment.html.replace(/\ue000MARKLITE_BLOCK_(\d+)\ue001/g, (_, index: string) =>
          `\ue000MARKLITE_BLOCK_${offset + Number(index) - segment.sourceBlockStart}\ue001`
        );
        return `<div class="preview-virtual-segment" data-preview-segment="${segment.index}">${html}</div>`;
      }).join('');
      const combinedHtml = `<div class="preview-virtual-spacer" data-preview-top style="height:${top}px"></div>` +
        windowHtml + `<div class="preview-virtual-spacer" data-preview-bottom style="height:${bottom}px"></div>`;
      schedulePreviewPreparation(
        combinedHtml,
        documentPath,
        settings.allowLocalImages,
        window.segments.flatMap((segment) => segment.diagrams),
        window.segments.flatMap((segment) => segment.diagramDiagnostics),
        diagramTheme
      );
    }).catch((error) => {
      if (activeVirtualPreview?.sessionId === current.sessionId && desiredVirtualWindow === key) {
        desiredVirtualWindow = '';
        const appError = toAppError(error);
        if (appError.code !== 'PREVIEW_SESSION_EXPIRED') uiActions.toast(localizeError(appError), 'error');
      }
    });
  }

  function updateVirtualWindowFromScroll() {
    if (!activeVirtualPreview || !previewHost || !activeVirtualPreview.segments.length) return;
    const index = previewSegmentAtPixel(virtualPrefix, previewHost.scrollTop);
    requestVirtualWindow(index);
  }

  function syncEditorFromUserScroll() {
    if (!settings.syncScroll || !previewHost || renderedRevision !== contentRevision ||
      committedIdentity !== `${tabId}\u0000${renderedRevision}` ||
      (!previewPointerActive && performance.now() >= userPreviewScrollUntil)) return;
    let line = sourceScrollMap ? findVisibleSourceBlockLine(sourceScrollMap, previewHost) : null;
    if (line === null && activeVirtualPreview?.segments.length) {
      const index = previewSegmentAtPixel(virtualPrefix, previewHost.scrollTop);
      line = activeVirtualPreview.segments[index].startLine;
    }
    if (line !== null) onSyncEditorLine(line);
  }

  function measureVirtualWindow() {
    if (!activeVirtualPreview || !previewHost || virtualWindowStart < 0) return;
    const previousTop = virtualPrefix[virtualWindowStart] ?? 0;
    let changed = false;
    for (const node of previewHost.querySelectorAll<HTMLElement>('[data-preview-segment]')) {
      const index = Number(node.dataset.previewSegment);
      if (!Number.isSafeInteger(index) || index < virtualWindowStart || index >= virtualWindowEnd) continue;
      const height = Math.max(1, node.getBoundingClientRect().height);
      if (Math.abs(virtualHeights[index] - height) > 0.5) {
        virtualHeights[index] = height;
        changed = true;
      }
    }
    if (!changed) return;
    virtualPrefix = previewHeightPrefix(virtualHeights);
    const top = previewHost.querySelector<HTMLElement>('[data-preview-top]');
    const bottom = previewHost.querySelector<HTMLElement>('[data-preview-bottom]');
    if (top) top.style.height = `${virtualPrefix[virtualWindowStart]}px`;
    if (bottom) bottom.style.height = `${virtualPrefix[virtualPrefix.length - 1] - virtualPrefix[virtualWindowEnd]}px`;
    previewHost.scrollTop += (virtualPrefix[virtualWindowStart] ?? 0) - previousTop;
    previewScrollRangeDirty = true;
  }

  function observePreviewHost(node: HTMLElement) {
    previewScrollRangeDirty = true;
    const onScroll = () => {
      if ((activeVirtualPreview || settings.syncScroll) && !virtualScrollFrame) {
        if (activeVirtualPreview && !pendingVirtualAnchor && !pendingFragment) scrollController.clear();
        virtualScrollFrame = requestAnimationFrame(() => {
          virtualScrollFrame = 0;
          updateVirtualWindowFromScroll();
          syncEditorFromUserScroll();
        });
      }
      if (previewChunks && !chunkSeenFrame) {
        chunkSeenFrame = requestAnimationFrame(() => {
          chunkSeenFrame = 0;
          markVisiblePreviewGroup(node);
        });
      }
    };
    node.addEventListener('scroll', onScroll, { passive: true });
    const onContentVisibility = (event: Event) => {
      // Nested skipped groups may become hittable after the scroll frame.
      // Reuse the bounded scheduler when the browser actually reveals them.
      if (previewChunks && (event as Event & { skipped: boolean }).skipped === false) onScroll();
    };
    node.addEventListener('contentvisibilityautostatechange', onContentVisibility, true);
    const markUserScroll = () => {
      userPreviewScrollUntil = performance.now() + 500;
      pendingVirtualAnchor = null;
      pendingFragment = null;
      scrollController.clear();
      cancelAnimationFrame(scrollCorrectionFrame);
    };
    const startPointerScroll = () => {
      previewPointerActive = true;
      markUserScroll();
    };
    const endPointerScroll = () => {
      if (!previewPointerActive) return;
      previewPointerActive = false;
      userPreviewScrollUntil = performance.now() + 500;
    };
    const onKeyScroll = (event: KeyboardEvent) => {
      if (['ArrowUp', 'ArrowDown', 'PageUp', 'PageDown', 'Home', 'End', ' '].includes(event.key)) markUserScroll();
    };
    node.addEventListener('wheel', markUserScroll, { passive: true });
    node.addEventListener('pointerdown', startPointerScroll, { passive: true });
    node.addEventListener('touchstart', startPointerScroll, { passive: true });
    node.addEventListener('keydown', onKeyScroll);
    window.addEventListener('pointerup', endPointerScroll);
    window.addEventListener('pointercancel', endPointerScroll);
    window.addEventListener('touchend', endPointerScroll);
    window.addEventListener('touchcancel', endPointerScroll);
    const observer = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(() => {
      if (activeVirtualPreview) {
        measureVirtualWindow();
        updateVirtualWindowFromScroll();
      }
      if (previewChunks) {
        cancelAnimationFrame(chunkScaleFrame);
        chunkScaleFrame = requestAnimationFrame(() => {
          if (previewChunks) rescalePreviewChunks(node, previewChunks);
          previewScrollRangeDirty = true;
          if (committedSettled) scrollController.layoutReady();
        });
      }
      previewScrollRangeDirty = true;
      if (!previewChunks && committedSettled) scrollController.layoutReady();
    });
    observer?.observe(node);
    const onResourceLayout = () => {
      if (activeVirtualPreview) measureVirtualWindow();
      previewScrollRangeDirty = true;
      if (committedSettled) scrollController.layoutReady();
    };
    node.addEventListener('load', onResourceLayout, true);
    node.addEventListener('error', onResourceLayout, true);
    return { destroy: () => {
      observer?.disconnect();
      node.removeEventListener('scroll', onScroll);
      node.removeEventListener('contentvisibilityautostatechange', onContentVisibility, true);
      node.removeEventListener('wheel', markUserScroll);
      node.removeEventListener('pointerdown', startPointerScroll);
      node.removeEventListener('touchstart', startPointerScroll);
      node.removeEventListener('keydown', onKeyScroll);
      window.removeEventListener('pointerup', endPointerScroll);
      window.removeEventListener('pointercancel', endPointerScroll);
      window.removeEventListener('touchend', endPointerScroll);
      window.removeEventListener('touchcancel', endPointerScroll);
      previewPointerActive = false;
      userPreviewScrollUntil = 0;
      node.removeEventListener('load', onResourceLayout, true);
      node.removeEventListener('error', onResourceLayout, true);
      previewScrollRangeDirty = true;
    } };
  }

  function markVisiblePreviewGroup(node: HTMLElement) {
    if (!previewChunks) return;
    const rect = node.getBoundingClientRect();
    const element = document.elementFromPoint?.(rect.left + rect.width / 2, rect.top + rect.height / 2);
    const group = element instanceof Element ? element.closest<HTMLElement>('.preview-chunk-group') : null;
    if (group && node.contains(group)) previewChunks.seenGroups.add(group);
  }

  function refreshPreviewScrollRange() {
    if (!previewHost) return;
    previewMaxScroll = Math.max(0, previewHost.scrollHeight - previewHost.clientHeight);
    previewScrollRangeDirty = false;
  }

  export function scrollToFragment(fragment: string) {
    userPreviewScrollUntil = 0;
    previewPointerActive = false;
    scrollController.clear();
    cancelAnimationFrame(scrollCorrectionFrame);
    if (previewMode !== 'article') {
      previewMode = 'article';
      window.setTimeout(() => scrollToFragment(fragment), 0);
      return;
    }
    const identity = `${tabId}\u0000${renderedRevision}`;
    if (!previewHost) {
      pendingFragment = { fragment, identity };
      return;
    }
    if (!fragment) {
      pendingFragment = null;
      previewHost.scrollTop = 0;
      return;
    }
    if (committedIdentity !== identity || !committedSettled) {
      pendingFragment = { fragment, identity };
      return;
    }
    const target = Array.from(previewHost.querySelectorAll<HTMLElement>('[id]')).find(
      (element) => element.id === fragment
    );
    if (target) {
      pendingFragment = null;
      if (previewChunks) {
        const align = () => {
          if (!previewHost?.contains(target)) return;
          previewHost.scrollTop += target.getBoundingClientRect().top - previewHost.getBoundingClientRect().top - 12;
        };
        align();
        requestAnimationFrame(() => {
          previewScrollRangeDirty = true;
        });
      } else {
        previewHost.scrollTop = Math.max(0, target.offsetTop - previewHost.offsetTop - 12);
      }
    } else if (activeVirtualPreview) {
      const heading = outline.find((item) => item.slug === fragment);
      if (!heading) {
        pendingFragment = null;
        uiActions.toast(t('toast.anchorMissing', { fragment }), 'error');
        return;
      }
      const index = previewSegmentAtLine(activeVirtualPreview.segments, heading.line);
      if (index >= virtualWindowStart && index < virtualWindowEnd) {
        pendingFragment = null;
        uiActions.toast(t('toast.anchorMissing', { fragment }), 'error');
        return;
      }
      pendingFragment = { fragment, identity };
      previewHost.scrollTop = virtualPrefix[index] ?? 0;
      requestVirtualWindow(index, true);
    } else {
      pendingFragment = null;
      uiActions.toast(t('toast.anchorMissing', { fragment }), 'error');
    }
  }

  async function setPreviewMode(mode: PreviewMode) {
    if (mode === previewMode) return;
    if (mode === 'article') {
      previewMode = mode;
      if (activeVirtualPreview) {
        desiredVirtualWindow = '';
        virtualWindowStart = -1;
        virtualWindowEnd = -1;
        void tick().then(() => requestVirtualWindow(0, true));
      }
      return;
    }
    try {
      MindMapPane = await mindMapLoader.load();
      resourceRevision += 1;
      cancelActiveResourceJob();
      previewDiagramRuntime.release();
      revokeObjectUrls(displayedObjectUrls);
      displayedObjectUrls = [];
      committedHtml = '';
      renderedNodes = [];
      committedWithoutImages = false;
      clearPreviewChunks();
      if (activeVirtualPreview) desiredVirtualWindow = '';
      sourceScrollMap = null;
      previewScrollRangeDirty = true;
      scheduledHtml = null;
      previewMode = mode;
    } catch (error) {
      uiActions.toast(t('toast.mindMapLoadFailed', { message: localizeError(toAppError(error)) }), 'error');
    }
  }

  async function handleClick(event: MouseEvent) {
    const target = event.target as HTMLElement | null;
    const anchor = target?.closest('a');
    if (!anchor) return;

    const href = anchor.getAttribute('href');
    if (!href) return;
    event.preventDefault();

    try {
      const resolved = await api.resolveMarkdownTarget(documentPath, href);
      await executeMarkdownTarget(resolved, settings.confirmExternalLinks, {
        scrollToFragment,
        openDocument: onOpenDocument,
        confirmExternal: (url) => window.confirm(t('confirm.externalLink', { url })),
        openExternal: openExternalLink,
        openEmail: openEmailLink
      });
    } catch (error) {
      uiActions.toast(localizeError(toAppError(error)), 'error');
    }
  }

  function handlePreviewKeydown(event: KeyboardEvent) {
    if (!activeVirtualPreview || event.key.toLowerCase() !== 'a' || !(event.ctrlKey || event.metaKey)) return;
    event.preventDefault();
    void onCopySource();
  }

  function schedulePreviewPreparation(
    currentHtml: string,
    currentDocumentPath: string | null,
    allowLocalImages: boolean,
    currentDiagrams: DiagramSource[],
    currentDiagramDiagnostics: DiagramDiagnostic[],
    currentDiagramTheme: DiagramTheme
  ) {
    resourceRevision += 1;
    committedSettled = false;
    scrollController.layoutPending();
    cancelActiveResourceJob();
    previewDiagramRuntime.cancel();
    const jobId = createResourceJobId();
    activeResourceJobId = jobId;
    void preparationQueue.submit({
      html: currentHtml,
      documentPath: currentDocumentPath,
      allowLocalImages,
      diagrams: currentDiagrams,
      diagramDiagnostics: currentDiagramDiagnostics,
      diagramTheme: currentDiagramTheme,
      fontFamily: settings.previewFontFamily,
      fontSize: settings.previewFontSize,
      lineHeight: settings.lineHeight,
      revision: resourceRevision,
      jobId
    }).catch((error) => uiActions.toast(localizeError(toAppError(error)), 'error'));
  }

  async function preparePreview(
    currentHtml: string,
    currentDocumentPath: string | null,
    allowLocalImages: boolean,
    currentDiagrams: DiagramSource[],
    currentDiagramDiagnostics: DiagramDiagnostic[],
    currentDiagramTheme: DiagramTheme,
    fontFamily: string,
    fontSize: number,
    lineHeight: number,
    revision: number,
    jobId: string
  ) {
    let preparedHtml = currentHtml;
    if (currentDiagrams.length > 0) {
      preparedHtml = await prepareDiagrams(
        currentHtml,
        currentDiagrams,
        currentDiagramDiagnostics,
        currentDiagramTheme,
        { fontFamily, fontSize, lineHeight },
        revision
      );
      if (revision !== resourceRevision) return;
    }
    if (!/<img\b/i.test(preparedHtml)) {
      commitPreparedPreview(preparedHtml, revision, [], true);
      return;
    }
    const template = createPreviewTemplate(preparedHtml);
    const images = Array.from(template.content.querySelectorAll<HTMLImageElement>('img[src]'));

    if (images.length === 0) {
      commitPreparedPreview(template.innerHTML, revision, []);
      return;
    }

    if (!allowLocalImages) {
      for (const image of images) {
        replaceWithImagePlaceholder(image, t('preview.image.disabled'));
      }
      commitPreparedPreview(template.innerHTML, revision, []);
      return;
    }

    const loadingTemplate = template.cloneNode(true) as HTMLTemplateElement;
    for (const image of loadingTemplate.content.querySelectorAll<HTMLImageElement>('img[src]')) {
      replaceWithImagePlaceholder(image, t('preview.image.loading'));
    }
    commitPreparedPreview(loadingTemplate.innerHTML, revision, [], false, false);

    const objectUrls = await prepareLocalImages(images, currentDocumentPath, revision, jobId);
    commitPreparedPreview(template.innerHTML, revision, objectUrls);
  }

  async function prepareDiagrams(
    currentHtml: string,
    currentDiagrams: DiagramSource[],
    currentDiagnostics: DiagramDiagnostic[],
    currentTheme: DiagramTheme,
    presentation: { fontFamily: string; fontSize: number; lineHeight: number },
    revision: number
  ): Promise<string> {
    const pendingTimer = window.setTimeout(() => {
      if (revision === resourceRevision) commitPreparedPreview(currentHtml, revision, [], true, false);
    }, 75);
    const template = createPreviewTemplate(currentHtml);
    try {
      const batch = await previewDiagramRuntime.render(
        currentDiagrams,
        currentDiagnostics,
        currentTheme,
        presentation
      );
      if (revision !== resourceRevision) return currentHtml;
      applyDiagramResults(template, currentDiagrams, batch.artifacts, batch.diagnostics);
    } catch (error) {
      if (revision !== resourceRevision) return currentHtml;
      const message = error instanceof Error ? error.message : String(error);
      applyDiagramResults(template, currentDiagrams, [], currentDiagrams.map((source) => ({
        code: 'DIAGRAM_RUNTIME_CRASHED',
        diagramId: source.diagramId,
        sourceStartByte: source.sourceStartByte,
        sourceEndByte: source.sourceEndByte,
        message,
        retryable: true
      })));
    } finally {
      window.clearTimeout(pendingTimer);
      previewDiagramRuntime.release();
    }
    return template.innerHTML;
  }

  function applyDiagramResults(
    template: HTMLTemplateElement,
    currentDiagrams: DiagramSource[],
    artifacts: RenderedDiagram[],
    diagnostics: DiagramDiagnostic[]
  ) {
    const codeBlocks = Array.from(
      template.content.querySelectorAll<HTMLElement>('pre > code.language-mermaid')
    );
    const artifactsById = new Map(artifacts.map((artifact) => [artifact.diagramId, artifact]));
    const diagnosticsById = new Map(diagnostics.map((diagnostic) => [diagnostic.diagramId, diagnostic]));
    currentDiagrams.forEach((source, index) => {
      const code = codeBlocks[index];
      if (!code) return;
      const artifact = artifactsById.get(source.diagramId);
      if (artifact) {
        const figure = createDiagramFigure(artifact, index);
        if (figure) {
          const pre = code.closest('pre');
          if (pre?.hasAttribute(SOURCE_BLOCK_ATTRIBUTE)) {
            figure.setAttribute(SOURCE_BLOCK_ATTRIBUTE, pre.getAttribute(SOURCE_BLOCK_ATTRIBUTE)!);
          }
          pre?.replaceWith(figure);
          return;
        }
      }
      const diagnostic = diagnosticsById.get(source.diagramId) ?? {
        code: artifact ? 'DIAGRAM_SVG_REJECTED' : 'DIAGRAM_OUTPUT_MISSING',
        diagramId: source.diagramId,
        sourceStartByte: source.sourceStartByte,
        sourceEndByte: source.sourceEndByte,
        message: artifact ? t('preview.diagram.svgInvalid') : t('preview.diagram.outputMissing'),
        retryable: true
      };
      appendDiagramDiagnostic(code, source, diagnostic, index);
    });
  }

  function createDiagramFigure(artifact: RenderedDiagram, index: number): HTMLElement | null {
    try {
      const parsed = new DOMParser().parseFromString(artifact.svgUtf8, 'image/svg+xml');
      const root = parsed.documentElement;
      if (root.localName !== 'svg' || parsed.querySelector('parsererror')) return null;
      const label = artifact.accessibleTitle?.trim() || t('preview.diagram.alt', { number: index + 1 });
      const description = artifact.accessibleDescription?.trim();
      const figure = document.createElement('figure');
      figure.className = 'mermaid-diagram';
      figure.dataset.diagramId = artifact.diagramId;
      figure.tabIndex = 0;
      figure.setAttribute('aria-label', label);
      const svg = document.importNode(root, true);
      svg.setAttribute('role', 'img');
      svg.setAttribute('aria-label', description ? `${label}. ${description}` : label);
      svg.style.width = `${artifact.width}px`;
      svg.style.minWidth = '100%';
      svg.style.height = 'auto';
      figure.append(svg);
      const caption = document.createElement('figcaption');
      caption.className = 'mermaid-diagram-caption';
      caption.textContent = description ? `${label}. ${description}` : label;
      figure.append(caption);
      return figure;
    } catch {
      return null;
    }
  }

  function appendDiagramDiagnostic(
    code: HTMLElement,
    source: DiagramSource,
    diagnostic: DiagramDiagnostic,
    index: number
  ) {
    const pre = code.closest('pre');
    if (!pre) return;
    pre.classList.add('mermaid-source-fallback');
    const note = document.createElement('div');
    note.className = 'mermaid-diagram-error';
    note.setAttribute('role', 'status');
    note.textContent = t('preview.diagram.error', {
      number: index + 1,
      start: source.sourceStartByte,
      end: source.sourceEndByte,
      message: diagnostic.message
    });
    pre.after(note);
  }

  function createPreviewTemplate(currentHtml: string): HTMLTemplateElement {
    const template = document.createElement('template');
    template.innerHTML = currentHtml;
    tagSourceBlockElements(template.content, activeVirtualPreview ? virtualWindowBlockStart : 0);
    if (activeVirtualPreview) {
      for (const segment of template.content.querySelectorAll<HTMLElement>('[data-preview-segment]')) {
        tagSourceBlockElements(segment, virtualWindowBlockStart);
      }
    }
    return template;
  }

  async function prepareLocalImages(
    images: HTMLImageElement[],
    currentDocumentPath: string | null,
    revision: number,
    jobId: string
  ): Promise<string[]> {
    const grouped = new Map<string, HTMLImageElement[]>();
    for (const image of images) {
      const source = image.getAttribute('src');
      if (!source) {
        replaceWithImagePlaceholder(image, t('preview.image.empty'));
        continue;
      }
      const group = grouped.get(source);
      if (group) group.push(image);
      else grouped.set(source, [image]);
    }
    const sources = [...grouped.keys()];
    let pendingObjectUrls: string[] = [];
    try {
      const batch = await api.loadLocalImages(currentDocumentPath, sources, jobId);
      pendingObjectUrls = batch.objectUrls;
      if (revision !== resourceRevision) {
        revokeObjectUrls(batch.objectUrls);
        return [];
      }
      if (
        batch.entries.length !== sources.length ||
        batch.entries.some((entry, index) => entry.target !== sources[index])
      ) {
        throw { code: 'INVALID_DESKTOP_CONTRACT', message: 'Preview image entries do not match the request' };
      }
      for (const entry of batch.entries) {
        const matchingImages = grouped.get(entry.target) ?? [];
        if (entry.error || !entry.resource) {
          const message = localizeError(toAppError(entry.error ?? {
            code: 'INVALID_DESKTOP_CONTRACT',
            message: 'Missing preview image resource'
          }));
          for (const image of matchingImages) replaceWithImagePlaceholder(image, message);
          continue;
        }
        for (const image of matchingImages) {
          image.title = entry.resource.path;
          image.setAttribute('loading', 'lazy');
          image.setAttribute('decoding', 'async');
          if (!image.hasAttribute('width')) image.width = entry.resource.width;
          if (!image.hasAttribute('height')) image.height = entry.resource.height;
          image.src = entry.resource.objectUrl;
        }
      }
      return pendingObjectUrls;
    } catch (error) {
      revokeObjectUrls(pendingObjectUrls);
      if (revision !== resourceRevision) return [];
      const message = localizeError(toAppError(error));
      for (const images of grouped.values()) {
        for (const image of images) replaceWithImagePlaceholder(image, message);
      }
      return [];
    }
  }

  function createResourceJobId(): string {
    const random = crypto.randomUUID?.() ?? `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
    return `preview-${random}`;
  }

  function cancelActiveResourceJob() {
    const jobId = activeResourceJobId;
    activeResourceJobId = null;
    if (jobId) void api.cancelLocalImageJob(jobId).catch(() => undefined);
  }

  function commitPreparedPreview(
    nextHtml: string,
    revision: number,
    nextObjectUrls: string[],
    withoutImages = false,
    settled = true
  ) {
    if (revision !== resourceRevision) {
      revokeObjectUrls(nextObjectUrls);
      return;
    }
    revokeObjectUrls(displayedObjectUrls);
    displayedObjectUrls = nextObjectUrls;
    if (!previewHost) return;
    const useChunks = !activeVirtualPreview && nextHtml.length >= PREVIEW_CHUNK_MIN_HTML_LENGTH;
    const virtualScrollTop = activeVirtualPreview ? previewHost.scrollTop : null;
    if (!nextHtml) {
      clearPreviewChunks();
      for (const node of renderedNodes) node.parentNode?.removeChild(node);
      renderedNodes = [];
    } else if (withoutImages && committedWithoutImages && !previewChunks && !useChunks && renderedNodes.length) {
      patchPreviewNodes(nextHtml);
    } else {
      clearPreviewChunks();
      const template = createPreviewTemplate(nextHtml);
      const chunks = useChunks ? wrapPreviewChunks(template.content) : null;
      const nextNodes = Array.from(template.content.childNodes);
      for (const node of renderedNodes) node.parentNode?.removeChild(node);
      previewHost.append(template.content);
      renderedNodes = nextNodes;
      if (chunks) {
        previewChunks = chunks;
        measurePreviewChunks(previewHost, chunks);
        chunkSeenFrame = requestAnimationFrame(() => {
          chunkSeenFrame = 0;
          markVisiblePreviewGroup(previewHost);
        });
        if (typeof ResizeObserver !== 'undefined') {
          chunkResizeObserver = new ResizeObserver(() => {
            previewScrollRangeDirty = true;
            if (committedSettled) scrollController.layoutReady();
          });
          for (const group of chunks.groups) chunkResizeObserver.observe(group);
        }
      }
    }
    if (virtualScrollTop !== null) previewHost.scrollTop = virtualScrollTop;
    committedHtml = nextHtml;
    committedWithoutImages = withoutImages;
    committedIdentity = `${tabId}\u0000${renderedRevision}`;
    committedSettled = settled;
    if (activeVirtualPreview) measureVirtualWindow();
    const articleElements = Array.from(previewHost.querySelectorAll<HTMLElement>(`[${SOURCE_BLOCK_ATTRIBUTE}]`));
    sourceScrollMap = buildSourceBlockScrollMap(activeVirtualPreview ? virtualWindowBlocks : sourceBlocks, articleElements);
    refreshPreviewScrollRange();
    if (settled) scrollController.layoutReady();
    if (settled && pendingVirtualAnchor && activeVirtualPreview) {
      const anchor = pendingVirtualAnchor;
      pendingVirtualAnchor = null;
      if (anchor.tabId === tabId && anchor.contentRevision === contentRevision) applyEditorScroll(anchor);
    }
    if (settled && pendingFragment) {
      if (pendingFragment.identity === committedIdentity) scrollToFragment(pendingFragment.fragment);
      else pendingFragment = null;
    }
  }

  function clearPreviewChunks() {
    cancelAnimationFrame(chunkScaleFrame);
    chunkScaleFrame = 0;
    cancelAnimationFrame(chunkSeenFrame);
    chunkSeenFrame = 0;
    chunkResizeObserver?.disconnect();
    chunkResizeObserver = null;
    previewChunks = null;
    sourceScrollMap = null;
    previewHost?.style.removeProperty('--preview-group-scale');
  }

  function patchPreviewNodes(nextHtml: string) {
    if (nextHtml === committedHtml) return;
    const template = createPreviewTemplate(nextHtml);
    renderedNodes = patchChildNodes(previewHost, renderedNodes, Array.from(template.content.childNodes));
  }

  function patchChildNodes(parent: Node, currentNodes: Node[], nextNodes: Node[]): Node[] {
    let first = 0;
    while (first < currentNodes.length && first < nextNodes.length && currentNodes[first].isEqualNode(nextNodes[first])) {
      first += 1;
    }
    let oldEnd = currentNodes.length;
    let newEnd = nextNodes.length;
    while (oldEnd > first && newEnd > first && currentNodes[oldEnd - 1].isEqualNode(nextNodes[newEnd - 1])) {
      oldEnd -= 1;
      newEnd -= 1;
    }
    const oldMiddle = currentNodes.slice(first, oldEnd);
    const newMiddle = nextNodes.slice(first, newEnd);
    let middle: Node[];
    if (oldMiddle.length === newMiddle.length && oldMiddle.every((node, index) => sameNodeKind(node, newMiddle[index]))) {
      for (let index = 0; index < oldMiddle.length; index += 1) patchNode(oldMiddle[index], newMiddle[index]);
      middle = oldMiddle;
    } else {
      for (const node of oldMiddle) parent.removeChild(node);
      const fragment = document.createDocumentFragment();
      fragment.append(...newMiddle);
      parent.insertBefore(fragment, currentNodes[oldEnd] ?? null);
      middle = newMiddle;
    }
    return [...currentNodes.slice(0, first), ...middle, ...currentNodes.slice(oldEnd)];
  }

  function sameNodeKind(current: Node, next: Node): boolean {
    return current.nodeType === next.nodeType && current.nodeName === next.nodeName &&
      (!(current instanceof Element && next instanceof Element) || current.namespaceURI === next.namespaceURI);
  }

  function patchNode(current: Node, next: Node) {
    if (current.isEqualNode(next)) return;
    if (current instanceof Element && next instanceof Element) {
      for (const { name } of Array.from(current.attributes)) {
        if (!next.hasAttribute(name)) current.removeAttribute(name);
      }
      for (const { name, value } of Array.from(next.attributes)) {
        if (current.getAttribute(name) !== value) current.setAttribute(name, value);
      }
      patchChildNodes(current, Array.from(current.childNodes), Array.from(next.childNodes));
    } else {
      current.nodeValue = next.nodeValue;
    }
  }

  function revokeObjectUrls(objectUrls: string[]) {
    for (const objectUrl of objectUrls) revokeObjectUrl(objectUrl);
  }

  function revokeObjectUrl(objectUrl: string) {
    if (objectUrl.startsWith('blob:')) URL.revokeObjectURL(objectUrl);
  }

  function replaceWithImagePlaceholder(image: HTMLImageElement, message: string) {
    const placeholder = document.createElement('span');
    placeholder.className = 'local-image-placeholder';
    placeholder.setAttribute('role', 'img');
    placeholder.textContent = `${image.alt || t('preview.image.alt')}: ${message}`;
    image.replaceWith(placeholder);
  }
</script>

<section class="preview" aria-label={$translator('preview.ariaLabel')}>
  <nav class="preview-mode-switcher" aria-label={$translator('preview.mode')}>
    <button
      type="button"
      class:active={previewMode === 'article'}
      aria-pressed={previewMode === 'article'}
      on:click={() => void setPreviewMode('article')}
    >{$translator('preview.article')}</button>
    <button
      type="button"
      class:active={previewMode === 'mindMap'}
      aria-pressed={previewMode === 'mindMap'}
      on:click={() => void setPreviewMode('mindMap')}
    >{$translator('preview.mindMap')}</button>
    {#if previewMode === 'article' && virtualPreview}
      <button type="button" on:click={() => void onCopySource()} title={$translator('preview.copySourceHint')}>
        {$translator('preview.copySource')}
      </button>
    {/if}
  </nav>

  <div class="preview-body">
    {#if previewMode === 'mindMap' && MindMapPane}
      <svelte:component
        this={MindMapPane}
        {documentTitle}
        {outline}
        {onJumpToLine}
      />
    {:else}
      <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
      <section
        bind:this={previewHost}
        use:observePreviewHost
        class="preview-content markdown-preview"
        class:virtual-preview={Boolean(activeVirtualPreview)}
        style:font-family={settings.previewFontFamily}
        style:font-size={`${settings.previewFontSize}px`}
        style:line-height={settings.lineHeight}
        on:click={handleClick}
        on:keydown={handlePreviewKeydown}
      >
        {#if !html && !virtualPreview}
          <div class="preview-empty">
            <h2>{$translator('preview.emptyTitle')}</h2>
            <p>{$translator('preview.emptyDescription')}</p>
          </div>
        {/if}
      </section>
    {/if}
  </div>
</section>
