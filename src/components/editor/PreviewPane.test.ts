/// <reference types="node" />

import { readFileSync } from 'node:fs';
import { mount, tick, unmount } from 'svelte';
import { get } from 'svelte/store';
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';
import PreviewPane from './PreviewPane.svelte';
import PreviewPaneHarness from '../../test/PreviewPaneHarness.svelte';
import type { EditorScrollPosition } from '../../app/stores/documentStore';
import { uiActions, uiStore } from '../../app/stores/uiStore';
import {
  defaultSettings,
  type AppSettings,
  type DiagramSource,
  type MarkdownTargetDto,
  type OutlineItem,
  type RenderedDiagram,
  type SanitizedMarkdownHtml,
  type VirtualPreviewIndex,
  type VirtualPreviewWindow
} from '../../lib/tauriApi';

const mocks = vi.hoisted(() => ({
  resolveMarkdownTarget: vi.fn(),
  loadLocalImage: vi.fn(),
  loadLocalImages: vi.fn(),
  cancelLocalImageJob: vi.fn(async () => undefined),
  renderDiagrams: vi.fn(),
  cancelDiagrams: vi.fn(),
  releaseDiagrams: vi.fn(),
  renderMarkdownWindow: vi.fn(),
  releaseMarkdownPreview: vi.fn(async () => undefined),
  openExternalLink: vi.fn(async () => undefined),
  openEmailLink: vi.fn(async () => undefined)
}));

vi.mock('../../lib/previewDiagramRuntime', () => ({
  previewDiagramRuntime: {
    render: mocks.renderDiagrams,
    cancel: mocks.cancelDiagrams,
    release: mocks.releaseDiagrams,
    cacheStats: vi.fn(() => ({ entries: 0, bytes: 0 }))
  }
}));

vi.mock('../../lib/tauriApi', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../lib/tauriApi')>();
  return {
    ...actual,
    api: {
      ...actual.api,
      resolveMarkdownTarget: mocks.resolveMarkdownTarget,
      loadLocalImages: mocks.loadLocalImages,
      cancelLocalImageJob: mocks.cancelLocalImageJob,
      renderMarkdownWindow: mocks.renderMarkdownWindow,
      releaseMarkdownPreview: mocks.releaseMarkdownPreview
    },
    openExternalLink: mocks.openExternalLink,
    openEmailLink: mocks.openEmailLink
  };
});

let target: HTMLDivElement;
let component: ReturnType<typeof mount> | undefined;
let previewStyle: HTMLStyleElement;
const previewCss = readFileSync('src/styles/markdown-preview.css', 'utf8');

function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

function settings(overrides: Partial<AppSettings> = {}): AppSettings {
  return { ...defaultSettings, ...overrides };
}

async function render(
  html: string,
  overrides: Partial<AppSettings> = {},
  onOpenDocument = vi.fn(async () => true),
  documentPath: string | null = 'C:\\docs\\index.md',
  outline: OutlineItem[] = [],
  onJumpToLine = vi.fn()
) {
  target = document.createElement('div');
  document.body.append(target);
  component = mount(PreviewPane, {
    target,
    props: {
      html: html as SanitizedMarkdownHtml,
      settings: settings(overrides),
      documentPath,
      documentTitle: 'Index.md',
      outline,
      onOpenDocument,
      onJumpToLine
    }
  });
  await tick();
  return { onOpenDocument };
}

function diagramSource(index: number, label = 'A-->B'): DiagramSource {
  const hash = (index + 1).toString(16).repeat(64).slice(0, 64);
  return {
    diagramId: `diagram-${index}-${hash.slice(0, 12)}`,
    ordinal: index,
    sourceUtf8: `flowchart TD\n${label}`,
    sourceSha256: hash,
    sourceStartByte: index * 30,
    sourceEndByte: index * 30 + 24
  };
}

function renderedDiagram(source: DiagramSource, label: string): RenderedDiagram {
  return {
    diagramId: source.diagramId,
    sourceSha256: source.sourceSha256,
    cacheKey: `cache-${source.diagramId}`,
    rendererId: 'mermaid-offline-11.17.2',
    svgUtf8: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 400 100"><title>${label}</title><text>${label}</text></svg>`,
    width: 400,
    height: 100,
    viewBox: [0, 0, 400, 100],
    accessibleTitle: label,
    accessibleDescription: `${label} description`,
    warnings: []
  };
}

describe('PreviewPane mind map mode', () => {
  test.each([false, true])('syncs user preview scrolling after paint without echoing editor scrolling (chunked=%s)', async (chunked) => {
    const syncEditorLine = vi.fn();
    target = document.createElement('div');
    document.body.append(target);
    component = mount(PreviewPane, { target, props: {
      html: (`\ue000MARKLITE_BLOCK_0\ue001<p>First${chunked ? 'x'.repeat(500_000) : ''}</p>\ue000MARKLITE_BLOCK_1\ue001<p><strong>Visible</strong></p>`) as SanitizedMarkdownHtml,
      tabId: 'ordinary', contentRevision: 1, renderedRevision: 1,
      sourceBlocks: [
        { startUtf16: 0, endUtf16: 20, startLine: 1, endLine: 2 },
        { startUtf16: 20, endUtf16: 40, startLine: 10, endLine: 11 }
      ],
      settings: settings({ syncScroll: true }), documentPath: null,
      documentTitle: 'Ordinary.md', outline: [], onOpenDocument: vi.fn(async () => true),
      onJumpToLine: vi.fn(), onSyncEditorLine: syncEditorLine
    } });
    await vi.waitFor(() => expect(target.querySelector('strong')).not.toBeNull());
    const host = target.querySelector<HTMLElement>('.preview-content')!;
    for (const block of host.querySelectorAll<HTMLElement>('[data-marklite-source-block]')) {
      block.scrollIntoView = vi.fn();
    }
    const original = Object.getOwnPropertyDescriptor(document, 'elementFromPoint');
    let painted = !chunked;
    Object.defineProperty(document, 'elementFromPoint', { configurable: true, value: vi.fn(() => painted ? target.querySelector('strong') : host) });
    vi.spyOn(host, 'getBoundingClientRect').mockReturnValue({ top: 0, left: 0, width: 500, height: 500, bottom: 500, right: 500 } as DOMRect);
    try {
      host.dispatchEvent(new WheelEvent('wheel'));
      host.dispatchEvent(new Event('scroll'));
      requestAnimationFrame(() => {
        painted = true;
        if (chunked) {
          const revealed = new Event('contentvisibilityautostatechange');
          Object.defineProperty(revealed, 'skipped', { value: false });
          host.querySelector('.preview-chunk')!.dispatchEvent(revealed);
        }
      });
      await vi.waitFor(() => expect(syncEditorLine).toHaveBeenCalledWith(10));
      syncEditorLine.mockClear();
      (component as typeof component & { syncToEditorScroll: (position: EditorScrollPosition) => void })
        .syncToEditorScroll({ line: 1, offsetUtf16: 0, ratio: 0, totalLines: 12, scrollTop: 0, scrollHeight: 1000, clientHeight: 500 });
      host.dispatchEvent(new Event('scroll'));
      await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      expect(syncEditorLine).not.toHaveBeenCalled();
      host.dispatchEvent(new Event('pointerdown'));
      const clock = vi.spyOn(performance, 'now').mockReturnValue(performance.now() + 1000);
      try {
        host.dispatchEvent(new Event('scroll'));
        await vi.waitFor(() => expect(syncEditorLine).toHaveBeenCalledWith(10));
      } finally {
        clock.mockRestore();
        window.dispatchEvent(new Event('pointerup'));
      }
    } finally {
      if (original) Object.defineProperty(document, 'elementFromPoint', original);
      else Reflect.deleteProperty(document, 'elementFromPoint');
    }
  });

  test('mounts only the requested virtual window and releases its session', async () => {
    const virtualPreview: VirtualPreviewIndex = {
      sessionId: 'virtual-test',
      segments: Array.from({ length: 20 }, (_, index) => ({
        startUtf16: index * 100,
        endUtf16: (index + 1) * 100,
        startLine: index + 1,
        endLine: index + 1,
        estimatedHeight: 300,
        estimatedNodes: 100
      }))
    };
    mocks.renderMarkdownWindow.mockImplementation(async (_sessionId: string, start: number, end: number) => ({
      sessionId: 'virtual-test', start, end,
      segments: Array.from({ length: end - start }, (_, offset) => {
        const index = start + offset;
        return {
          index,
          html: `\ue000MARKLITE_BLOCK_${index === 19 ? 18 : index}\ue001${index === 19 ? '<h2 id="last">Segment 19</h2>' : `<p>Segment ${index}</p>`}`,
          sourceBlockStart: index === 19 ? 18 : index,
          sourceBlocks: [{ startUtf16: index * 100, endUtf16: (index + 1) * 100, startLine: index + 1, endLine: index + 1 }],
          diagrams: [], diagramDiagnostics: []
        };
      })
    }));
    target = document.createElement('div');
    document.body.append(target);
    const copySource = vi.fn(async () => undefined);
    const syncEditorLine = vi.fn();
    component = mount(PreviewPane, {
      target,
      props: {
        html: '' as SanitizedMarkdownHtml,
        tabId: 'virtual-tab', contentRevision: 1, renderedRevision: 1,
        virtualPreview, sourceBlocks: [], settings: settings({ syncScroll: true }), documentPath: null,
        documentTitle: 'Large.md', outline: [{ level: 2, title: 'Segment 19', line: 20, slug: 'last' }], onOpenDocument: vi.fn(async () => true),
        onJumpToLine: vi.fn(), onSyncEditorLine: syncEditorLine, onCopySource: copySource
      }
    });
    await vi.waitFor(() => expect(target.querySelector('[data-preview-segment="0"]')).not.toBeNull());
    const host = target.querySelector<HTMLElement>('.preview-content')!;
    expect(host.textContent).toContain('Segment 0');
    expect(host.querySelectorAll('[data-preview-segment]').length).toBeLessThanOrEqual(8);
    // Browsers can clamp a scroller while the old window is removed before the new one is attached.
    const removeChild = host.removeChild.bind(host);
    vi.spyOn(host, 'removeChild').mockImplementation((node) => {
      const removed = removeChild(node);
      host.scrollTop = 0;
      return removed;
    });
    const copyButton = Array.from(target.querySelectorAll('button')).find((button) => button.textContent?.includes('Copy full source'))!;
    copyButton.click();
    expect(copySource).toHaveBeenCalledTimes(1);
    host.scrollTop = 19 * 300;
    host.dispatchEvent(new WheelEvent('wheel'));
    host.dispatchEvent(new Event('scroll'));
    await vi.waitFor(() => expect(host.textContent).toContain('Segment 19'));
    expect(host.scrollTop).toBe(19 * 300);
    expect(host.textContent).not.toContain('Segment 0');
    expect(host.querySelectorAll('[data-preview-segment]').length).toBeLessThanOrEqual(8);
    const blockTags = Array.from(host.querySelectorAll('[data-marklite-source-block]'));
    expect(blockTags.map((element) => element.getAttribute('data-marklite-source-block')))
      .toEqual(blockTags.map((_, index) => String(index)));
    expect(syncEditorLine).toHaveBeenCalledWith(20);
    (component as typeof component & { syncToEditorScroll: (position: EditorScrollPosition) => void })
      .syncToEditorScroll({ line: 1, offsetUtf16: 10, ratio: 0, totalLines: 20, scrollTop: 0, scrollHeight: 6000, clientHeight: 600 });
    await vi.waitFor(() => expect(host.textContent).toContain('Segment 0'));
    (component as typeof component & { scrollToFragment: (fragment: string) => void }).scrollToFragment('last');
    await vi.waitFor(() => expect(host.querySelector('#last')).not.toBeNull());
    await unmount(component);
    component = undefined;
    expect(mocks.releaseMarkdownPreview).toHaveBeenCalledWith('virtual-test');
  });

  test('uses the latest user position over older windows and editor anchors', async () => {
    const virtualPreview: VirtualPreviewIndex = {
      sessionId: 'virtual-race',
      segments: Array.from({ length: 20 }, (_, index) => ({
        startUtf16: index * 100, endUtf16: (index + 1) * 100,
        startLine: index + 1, endLine: index + 1,
        estimatedHeight: 300, estimatedNodes: 10
      }))
    };
    const windowFor = (start: number, end: number): VirtualPreviewWindow => ({
      sessionId: 'virtual-race', start, end,
      segments: Array.from({ length: end - start }, (_, offset) => {
        const index = start + offset;
        return {
          index, sourceBlockStart: index,
          html: `\ue000MARKLITE_BLOCK_${index}\ue001<p>Segment ${index}</p>` as SanitizedMarkdownHtml,
          sourceBlocks: [{ startUtf16: index * 100, endUtf16: (index + 1) * 100, startLine: index + 1, endLine: index + 1 }],
          diagrams: [], diagramDiagnostics: []
        };
      })
    });
    const delayed = deferred<VirtualPreviewWindow>();
    mocks.renderMarkdownWindow.mockImplementation(async (_sessionId: string, start: number, end: number) =>
      start === 9 ? delayed.promise : windowFor(start, end)
    );
    target = document.createElement('div');
    document.body.append(target);
    component = mount(PreviewPane, { target, props: {
      html: '' as SanitizedMarkdownHtml, tabId: 'virtual-race-tab',
      contentRevision: 1, renderedRevision: 1, virtualPreview, sourceBlocks: [],
      settings: settings({ syncScroll: true }), documentPath: null,
      documentTitle: 'Large.md', outline: [], onOpenDocument: vi.fn(async () => true),
      onJumpToLine: vi.fn()
    } });
    await vi.waitFor(() => expect(target.querySelector('[data-preview-segment="0"]')).not.toBeNull());
    const host = target.querySelector<HTMLElement>('.preview-content')!;
    host.scrollTop = 10 * 300;
    host.dispatchEvent(new Event('scroll'));
    await vi.waitFor(() => expect(mocks.renderMarkdownWindow).toHaveBeenCalledWith('virtual-race', 9, 11));
    host.scrollTop = 19 * 300;
    delayed.resolve(windowFor(9, 11));
    await vi.waitFor(() => expect(host.querySelector('[data-preview-segment="19"]')).not.toBeNull());
    expect(host.querySelector('[data-preview-segment="10"]')).toBeNull();
    expect(host.scrollTop).toBe(19 * 300);

    const delayedEditorAnchor = deferred<VirtualPreviewWindow>();
    mocks.renderMarkdownWindow.mockImplementation(async (_sessionId: string, start: number, end: number) =>
      start === 9 ? delayedEditorAnchor.promise : windowFor(start, end)
    );
    (component as typeof component & { syncToEditorScroll: (position: EditorScrollPosition) => void })
      .syncToEditorScroll({ line: 11, offsetUtf16: 1000, ratio: 0.5, totalLines: 20, scrollTop: 3000, scrollHeight: 6000, clientHeight: 600 });
    await vi.waitFor(() => expect(host.scrollTop).toBeLessThan(19 * 300));
    await vi.waitFor(() => expect(mocks.renderMarkdownWindow.mock.calls.filter(([, start]) => start === 9)).toHaveLength(2));
    host.scrollTop = 19 * 300;
    host.dispatchEvent(new WheelEvent('wheel'));
    delayedEditorAnchor.resolve(windowFor(9, 11));
    await vi.waitFor(() => expect(mocks.renderMarkdownWindow.mock.calls.slice(4)
      .some(([, start, end]) => start > 10 && end === 20)).toBe(true));
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(host.scrollTop).toBe(19 * 300);
    expect(mocks.renderMarkdownWindow.mock.calls.filter(([, start]) => start === 9)).toHaveLength(2);
  });

  test('releases an image URL when its virtual segment leaves the window', async () => {
    const originalDescriptor = Object.getOwnPropertyDescriptor(URL, 'revokeObjectURL');
    const revokeObjectUrl = vi.fn();
    Object.defineProperty(URL, 'revokeObjectURL', { configurable: true, value: revokeObjectUrl });
    try {
      mocks.loadLocalImage.mockResolvedValue({ objectUrl: 'blob:virtual-image', path: 'C:\\docs\\image.png' });
      const virtualPreview: VirtualPreviewIndex = {
        sessionId: 'virtual-image',
        segments: Array.from({ length: 20 }, (_, index) => ({
          startUtf16: index * 100, endUtf16: (index + 1) * 100,
          startLine: index + 1, endLine: index + 1,
          estimatedHeight: 300, estimatedNodes: 10
        }))
      };
      mocks.renderMarkdownWindow.mockImplementation(async (_sessionId: string, start: number, end: number) => ({
        sessionId: 'virtual-image', start, end,
        segments: Array.from({ length: end - start }, (_, offset) => {
          const index = start + offset;
          return {
            index, sourceBlockStart: index,
            html: `\ue000MARKLITE_BLOCK_${index}\ue001${index === 0 ? '<p><img src="marklite:image.png" alt="Image"></p>' : `<p>Segment ${index}</p>`}`,
            sourceBlocks: [{ startUtf16: index * 100, endUtf16: (index + 1) * 100, startLine: index + 1, endLine: index + 1 }],
            diagrams: [], diagramDiagnostics: []
          };
        })
      }));
      target = document.createElement('div');
      document.body.append(target);
      component = mount(PreviewPane, { target, props: {
        html: '' as SanitizedMarkdownHtml, tabId: 'virtual-image-tab',
        contentRevision: 1, renderedRevision: 1, virtualPreview, sourceBlocks: [],
        settings: settings({ allowLocalImages: true }), documentPath: 'C:\\docs\\index.md',
        documentTitle: 'Large.md', outline: [], onOpenDocument: vi.fn(async () => true),
        onJumpToLine: vi.fn()
      } });
      await vi.waitFor(() => expect(target.querySelector('img')?.getAttribute('src')).toBe('blob:virtual-image'));
      const host = target.querySelector<HTMLElement>('.preview-content')!;
      host.scrollTop = 19 * 300;
      host.dispatchEvent(new Event('scroll'));
      await vi.waitFor(() => expect(host.textContent).toContain('Segment 19'));
      await vi.waitFor(() => expect(revokeObjectUrl).toHaveBeenCalledWith('blob:virtual-image'));
    } finally {
      if (originalDescriptor) Object.defineProperty(URL, 'revokeObjectURL', originalDescriptor);
      else Reflect.deleteProperty(URL, 'revokeObjectURL');
    }
  });

  test('waits for the matching article commit before resolving a fragment', async () => {
    const toast = vi.spyOn(uiActions, 'toast');
    target = document.createElement('div');
    document.body.append(target);
    component = mount(PreviewPane, {
      target,
      props: {
        html: '\ue000MARKLITE_BLOCK_0\ue001<h1 id="later">Later</h1>' as SanitizedMarkdownHtml,
        tabId: 'fragment-tab',
        contentRevision: 1,
        renderedRevision: 1,
        sourceBlocks: [{ startUtf16: 0, endUtf16: 7, startLine: 1, endLine: 1 }],
        settings: settings(),
        documentPath: null,
        documentTitle: 'Fragment.md',
        outline: [],
        onOpenDocument: vi.fn(async () => true),
        onJumpToLine: vi.fn()
      }
    });
    (component as typeof component & { scrollToFragment: (fragment: string) => void }).scrollToFragment('later');
    await vi.waitFor(() => expect(target.querySelector('#later')).not.toBeNull());
    expect(toast).not.toHaveBeenCalled();
    expect(target.textContent).not.toContain('MARKLITE_BLOCK');
    toast.mockRestore();
  });

  test('keeps large image previews chunked while a resource settles', async () => {
    const pending = deferred<{ objectUrl: string; path: string }>();
    mocks.loadLocalImage.mockReturnValue(pending.promise);
    const html = '<p><img src="marklite:large.png" alt="Large"></p><p>' + 'x'.repeat(500_000) + '</p>';
    await render(html, { allowLocalImages: true });
    await vi.waitFor(() => expect(target.querySelector('.local-image-placeholder')).not.toBeNull());
    expect(target.querySelector('.preview-chunk')).not.toBeNull();
    pending.resolve({ objectUrl: 'data:image/png;base64,cG5n', path: 'C:\\docs\\large.png' });
    await vi.waitFor(() => expect(target.querySelector('img')).not.toBeNull());
    expect(target.querySelector('.preview-chunk')).not.toBeNull();
  });

  test('reuses preview scroll range across editor position updates', async () => {
    await render('<p>One</p>');
    const article = target.querySelector<HTMLElement>('.markdown-preview')!;
    await vi.waitFor(() => expect(article.textContent).toContain('One'));
    const scrollHeight = vi.spyOn(article, 'scrollHeight', 'get');
    const position: EditorScrollPosition = { line: 1, ratio: 0.5, totalLines: 1, scrollTop: 0, scrollHeight: 100, clientHeight: 50 };
    const preview = component as typeof component & { syncToEditorScroll: (position: EditorScrollPosition) => void };

    preview.syncToEditorScroll(position);
    preview.syncToEditorScroll({ ...position, ratio: 0.75 });

    expect(scrollHeight).toHaveBeenCalledTimes(0);
  });

  test('keeps article scrolling and mind map inside one bounded native-size body', async () => {
    await render(
      '<h1 id="plan">Plan</h1>',
      {},
      vi.fn(async () => true),
      'C:\\docs\\index.md',
      [{ level: 1, title: 'Plan', line: 7, slug: 'plan' }]
    );
    const surface = target.querySelector<HTMLElement>('.preview-body')!;
    const article = surface.querySelector<HTMLElement>('.markdown-preview')!;
    expect(surface.style.transform).toBe('');
    expect(surface.hasAttribute('data-content-scale')).toBe(false);
    expect(getComputedStyle(surface).overflow).toBe('hidden');
    expect(getComputedStyle(article).overflow).toBe('auto');

    [...target.querySelectorAll<HTMLButtonElement>('.preview-mode-switcher button')]
      .find((button) => button.textContent?.trim() === 'Mind map')!
      .click();
    await vi.waitFor(() => expect(surface.querySelector('.mind-map')).not.toBeNull());
    expect(target.querySelectorAll('.preview-body')).toHaveLength(1);
  });

  test('loads the mind map on demand and routes node activation back to the editor', async () => {
    const onJumpToLine = vi.fn();
    await render(
      '<h1 id="plan">Plan</h1>',
      {},
      vi.fn(async () => true),
      'C:\\docs\\index.md',
      [{ level: 1, title: 'Plan', line: 7, slug: 'plan' }],
      onJumpToLine
    );

    [...target.querySelectorAll<HTMLButtonElement>('.preview-mode-switcher button')]
      .find((button) => button.textContent?.trim() === 'Mind map')!
      .click();
    await vi.waitFor(() => expect(target.querySelector('.mind-map-node')).not.toBeNull());

    [...target.querySelectorAll<HTMLButtonElement>('.mind-map-node-label')]
      .find((button) => button.textContent?.includes('Plan'))!
      .click();
    expect(onJumpToLine).toHaveBeenCalledWith(7);
    expect(target.querySelector('.markdown-preview')).toBeNull();
  });
});

async function click(selector: string) {
  const element = target.querySelector<HTMLElement>(selector);
  expect(element).not.toBeNull();
  element!.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
  await vi.waitFor(() => expect(mocks.resolveMarkdownTarget).toHaveBeenCalled());
}

beforeEach(() => {
  previewStyle = document.createElement('style');
  previewStyle.textContent = previewCss;
  document.head.append(previewStyle);
  mocks.resolveMarkdownTarget.mockReset();
  mocks.loadLocalImage.mockReset();
  mocks.loadLocalImages.mockReset();
  mocks.loadLocalImages.mockImplementation(async (documentPath: string | null, targets: string[], jobId: string) => {
    const entries = await Promise.all(targets.map(async (target) => {
      try {
        const resource = await mocks.loadLocalImage(documentPath, target, jobId);
        return {
          target,
          resource: { ...resource, width: 1, height: 1, encodedBytes: 1, decodedBytes: 4 },
          error: null
        };
      } catch (error) {
        return { target, resource: null, error };
      }
    }));
    return {
      entries,
      objectUrls: [...new Set(entries.flatMap((entry) => entry.resource ? [entry.resource.objectUrl] : []))]
    };
  });
  mocks.cancelLocalImageJob.mockReset();
  mocks.cancelLocalImageJob.mockResolvedValue(undefined);
  mocks.renderDiagrams.mockReset();
  mocks.renderDiagrams.mockResolvedValue({ artifacts: [], diagnostics: [] });
  mocks.cancelDiagrams.mockReset();
  mocks.releaseDiagrams.mockReset();
  mocks.openExternalLink.mockReset();
  mocks.openEmailLink.mockReset();
  vi.spyOn(window, 'confirm').mockReturnValue(true);
});

afterEach(async () => {
  if (component) await unmount(component);
  component = undefined;
  target?.remove();
  previewStyle.remove();
});

describe('PreviewPane Markdown navigation', () => {
  test('routes a local document through the App callback and never through opener', async () => {
    const resolved: MarkdownTargetDto = {
      kind: 'localDocument',
      path: 'C:\\docs\\other.md',
      fragment: 'part'
    };
    mocks.resolveMarkdownTarget.mockResolvedValue(resolved);
    const { onOpenDocument } = await render('<p><a href="marklite:other%2Emd">Other</a></p>');

    await click('a');
    await vi.waitFor(() =>
      expect(onOpenDocument).toHaveBeenCalledWith('C:\\docs\\other.md', 'part')
    );

    expect(mocks.resolveMarkdownTarget).toHaveBeenCalledWith(
      'C:\\docs\\index.md',
      'marklite:other%2Emd'
    );
    expect(mocks.openExternalLink).not.toHaveBeenCalled();
    expect(window.confirm).not.toHaveBeenCalled();
  });

  test('asks only for external confirmation and cancellation never reaches opener', async () => {
    mocks.resolveMarkdownTarget.mockResolvedValue({
      kind: 'external',
      url: 'https://example.com/path'
    } satisfies MarkdownTargetDto);
    vi.mocked(window.confirm).mockReturnValue(false);
    await render('<a href="marklite:https%3A%2F%2Fexample%2Ecom">External</a>');

    await click('a');
    await vi.waitFor(() => expect(window.confirm).toHaveBeenCalledTimes(1));

    expect(mocks.openExternalLink).not.toHaveBeenCalled();
  });

  test('keeps same-document anchors internal', async () => {
    mocks.resolveMarkdownTarget.mockResolvedValue({
      kind: 'anchor',
      fragment: '标题'
    } satisfies MarkdownTargetDto);
    await render('<h2 id="标题">标题</h2><a href="marklite:%23%E6%A0%87%E9%A2%98">Jump</a>');

    await vi.waitFor(() => expect(target.querySelector('h2')?.id).toBe('标题'));
    await click('a');

    expect(mocks.openExternalLink).not.toHaveBeenCalled();
    expect(window.confirm).not.toHaveBeenCalled();
  });

  test('routes email through the controlled mail opener', async () => {
    mocks.resolveMarkdownTarget.mockResolvedValue({
      kind: 'email',
      address: 'writer@example.org'
    } satisfies MarkdownTargetDto);
    await render('<a href="marklite:mailto%3Awriter%40example%2Eorg">Email</a>');

    await click('a');
    await vi.waitFor(() => expect(mocks.openEmailLink).toHaveBeenCalledWith('writer@example.org'));

    expect(window.confirm).toHaveBeenCalledWith(expect.stringContaining('mailto:writer@example.org'));
    expect(mocks.openExternalLink).not.toHaveBeenCalled();
  });

  test('shows the structured save-first error for a relative target in an unsaved document', async () => {
    mocks.resolveMarkdownTarget.mockRejectedValue({
      code: 'RELATIVE_TARGET_REQUIRES_SAVED_DOCUMENT',
      message: '相对链接需要先保存当前文档'
    });
    await render('<a href="marklite:other%2Emd">Other</a>', {}, vi.fn(), null);

    await click('a');
    await vi.waitFor(() =>
      expect(get(uiStore).toasts.at(-1)).toMatchObject({
        message: 'Save the current document before opening a relative local path.',
        tone: 'error'
      })
    );
    expect(mocks.resolveMarkdownTarget).toHaveBeenCalledWith(null, 'marklite:other%2Emd');
    expect(mocks.openExternalLink).not.toHaveBeenCalled();
  });
});

describe('PreviewPane local images', () => {
  test('replaces the empty state when the first rendered article arrives', async () => {
    target = document.createElement('div');
    document.body.append(target);
    component = mount(PreviewPaneHarness, {
      target,
      props: {
        html: '' as SanitizedMarkdownHtml,
        alternateHtml: '<h1 id="first">First</h1>' as SanitizedMarkdownHtml,
        documentPath: 'C:\\docs\\index.md'
      }
    });
    await vi.waitFor(() => expect(target.querySelector('.preview-empty')).not.toBeNull());
    target.querySelector<HTMLButtonElement>('[data-testid="switch-html"]')!.click();
    await vi.waitFor(() => expect(target.querySelector('#first')?.textContent).toBe('First'));
    expect(target.querySelector('.preview-empty')).toBeNull();
  });

  test('restores the empty state after clearing a rendered article', async () => {
    target = document.createElement('div');
    document.body.append(target);
    component = mount(PreviewPaneHarness, {
      target,
      props: {
        html: '<h1 id="first">First</h1>' as SanitizedMarkdownHtml,
        alternateHtml: '' as SanitizedMarkdownHtml,
        documentPath: 'C:\\docs\\index.md'
      }
    });
    await vi.waitFor(() => expect(target.querySelector('#first')).not.toBeNull());
    target.querySelector<HTMLButtonElement>('[data-testid="switch-html"]')!.click();
    await vi.waitFor(() => expect(target.querySelector('.preview-empty')).not.toBeNull());
    expect(target.querySelector('#first')).toBeNull();
  });

  test('preserves unchanged preview blocks when a nearby block changes', async () => {
    target = document.createElement('div');
    document.body.append(target);
    component = mount(PreviewPaneHarness, {
      target,
      props: {
        html: '<p id="edited">Before</p><p id="stable">Keep me</p>' as SanitizedMarkdownHtml,
        alternateHtml: '<p id="edited">After</p><p id="stable">Keep me</p>' as SanitizedMarkdownHtml,
        documentPath: 'C:\\docs\\index.md'
      }
    });
    await vi.waitFor(() => expect(target.querySelector('#stable')).not.toBeNull());
    const stable = target.querySelector('#stable');
    const edited = target.querySelector('#edited');
    target.querySelector<HTMLButtonElement>('[data-testid="switch-html"]')!.click();
    await vi.waitFor(() => expect(target.querySelector('#edited')?.textContent).toBe('After'));
    expect(target.querySelector('#edited')).toBe(edited);
    expect(target.querySelector('#stable')).toBe(stable);
  });

  test('does not invoke the backend reader when local images are disabled', async () => {
    await render('<img src="marklite:images%2Fpixel%2Epng" alt="Pixel">', {
      allowLocalImages: false
    });

    await vi.waitFor(() =>
      expect(target.querySelector('.local-image-placeholder')?.textContent).toContain(
        'Local images are disabled'
      )
    );
    expect(mocks.loadLocalImage).not.toHaveBeenCalled();
    expect(target.querySelector('img')).toBeNull();
  });

  test('uses only the validated object URL returned by the backend', async () => {
    mocks.loadLocalImage.mockResolvedValue({
      objectUrl: 'data:image/png;base64,cG5n',
      path: 'C:\\docs\\images\\pixel.png'
    });
    await render('<img src="marklite:images%2Fpixel%2Epng" alt="Pixel">', {
      allowLocalImages: true
    });

    await vi.waitFor(() =>
      expect(target.querySelector<HTMLImageElement>('img')?.src).toBe(
        'data:image/png;base64,cG5n'
      )
    );
    expect(mocks.loadLocalImage).toHaveBeenCalledWith(
      'C:\\docs\\index.md',
      'marklite:images%2Fpixel%2Epng',
      expect.stringMatching(/^preview-/)
    );
    expect(target.querySelector<HTMLImageElement>('img')?.title).toBe(
      'C:\\docs\\images\\pixel.png'
    );
  });

  test('replaces a failed image with the structured backend message', async () => {
    mocks.loadLocalImage.mockRejectedValue({
      code: 'FILE_NOT_FOUND',
      message: '文件不存在：missing.png'
    });
    await render('<img src="marklite:missing%2Epng" alt="Missing">', {
      allowLocalImages: true
    });

    await vi.waitFor(() =>
      expect(target.querySelector('.local-image-placeholder')?.textContent).toContain(
        'The file does not exist.'
      )
    );
    expect(target.querySelector('img')).toBeNull();
  });

  test('shows a deterministic placeholder when the document image budget is exhausted', async () => {
    mocks.loadLocalImages.mockResolvedValueOnce({
      entries: [{
        target: 'marklite:huge%2Epng',
        resource: null,
        error: {
          code: 'PREVIEW_IMAGE_BUDGET_EXCEEDED',
          message: '预览图片超过当前文档的资源预算，已停止继续加载'
        }
      }],
      objectUrls: []
    });
    await render('<img src="marklite:huge%2Epng" alt="Huge">', { allowLocalImages: true });

    await vi.waitFor(() =>
      expect(target.querySelector('.local-image-placeholder')?.textContent).toContain(
        'Preview image resources exceed the per-document budget'
      )
    );
    expect(target.querySelector('img')).toBeNull();
    expect(mocks.loadLocalImages).toHaveBeenCalledOnce();
  });

  test('restores the original image target when permission is enabled without editing Markdown', async () => {
    mocks.loadLocalImage.mockResolvedValue({
      objectUrl: 'data:image/png;base64,cG5n',
      path: 'C:\\docs\\images\\pixel.png'
    });
    target = document.createElement('div');
    document.body.append(target);
    component = mount(PreviewPaneHarness, {
      target,
      props: {
        html: '<img src="marklite:images%2Fpixel%2Epng" alt="Pixel">' as SanitizedMarkdownHtml,
        documentPath: 'C:\\docs\\index.md'
      }
    });
    await vi.waitFor(() =>
      expect(target.querySelector('.local-image-placeholder')).not.toBeNull()
    );

    target
      .querySelector<HTMLButtonElement>('[data-testid="enable-images"]')!
      .dispatchEvent(new MouseEvent('click', { bubbles: true }));

    await vi.waitFor(() =>
      expect(target.querySelector<HTMLImageElement>('img')?.src).toBe(
        'data:image/png;base64,cG5n'
      )
    );
    expect(mocks.loadLocalImage).toHaveBeenCalledWith(
      'C:\\docs\\index.md',
      'marklite:images%2Fpixel%2Epng',
      expect.stringMatching(/^preview-/)
    );
  });

  test('keeps the current image resolvable when settings refresh during a pending load', async () => {
    const pending = deferred<{ objectUrl: string; path: string }>();
    mocks.loadLocalImage.mockReturnValue(pending.promise);
    target = document.createElement('div');
    document.body.append(target);
    component = mount(PreviewPaneHarness, {
      target,
      props: {
        html: '<img src="marklite:images%2F%E4%B8%AD%E6%96%87%20pixel%2Epng" alt="中文图片">' as SanitizedMarkdownHtml,
        documentPath: 'C:\\docs\\index.md',
        initialAllowLocalImages: true
      }
    });

    await vi.waitFor(() => expect(mocks.loadLocalImage).toHaveBeenCalledTimes(1));
    expect(target.querySelector('img')).toBeNull();
    expect(target.querySelector('.local-image-placeholder')?.textContent).toContain(
      'Loading local image'
    );

    target
      .querySelector<HTMLButtonElement>('[data-testid="refresh-settings"]')!
      .dispatchEvent(new MouseEvent('click', { bubbles: true }));
    await tick();

    pending.resolve({
      objectUrl: 'data:image/png;base64,Y3VycmVudA==',
      path: 'C:\\docs\\images\\中文 pixel.png'
    });

    await vi.waitFor(() =>
      expect(target.querySelector<HTMLImageElement>('img')?.getAttribute('src')).toBe(
        'data:image/png;base64,Y3VycmVudA=='
      )
    );
    expect(mocks.loadLocalImage).toHaveBeenCalledTimes(1);
  });

  test('never lets stale success or failure mutate a newer preview revision', async () => {
    const staleSuccess = deferred<{ objectUrl: string; path: string }>();
    const staleFailure = deferred<{ objectUrl: string; path: string }>();
    mocks.loadLocalImage
      .mockReturnValueOnce(staleSuccess.promise)
      .mockReturnValueOnce(staleFailure.promise)
      .mockResolvedValueOnce({
        objectUrl: 'data:image/png;base64,bmV3',
        path: 'C:\\docs\\images\\new.png'
      });
    target = document.createElement('div');
    document.body.append(target);
    component = mount(PreviewPaneHarness, {
      target,
      props: {
        html: [
          '<img src="marklite:images%2Fold-success%2Epng" alt="Old success">',
          '<img src="marklite:images%2Fold-failure%2Epng" alt="Old failure">'
        ].join('') as SanitizedMarkdownHtml,
        alternateHtml: '<img src="marklite:images%2Fnew%2Epng" alt="New">' as SanitizedMarkdownHtml,
        documentPath: 'C:\\docs\\index.md',
        initialAllowLocalImages: true
      }
    });

    await vi.waitFor(() => expect(mocks.loadLocalImage).toHaveBeenCalledTimes(2));
    target
      .querySelector<HTMLButtonElement>('[data-testid="switch-html"]')!
      .dispatchEvent(new MouseEvent('click', { bubbles: true }));
    await tick();
    expect(mocks.loadLocalImage).toHaveBeenCalledTimes(2);

    staleSuccess.resolve({
      objectUrl: 'data:image/png;base64,b2xk',
      path: 'C:\\docs\\images\\old-success.png'
    });
    staleFailure.reject({ code: 'FILE_NOT_FOUND', message: '旧图片不存在' });
    await vi.waitFor(() => expect(mocks.loadLocalImage).toHaveBeenCalledTimes(3));
    await vi.waitFor(() =>
      expect(target.querySelector<HTMLImageElement>('img')?.getAttribute('src')).toBe(
        'data:image/png;base64,bmV3'
      )
    );

    expect(target.querySelectorAll('img')).toHaveLength(1);
    expect(target.querySelector<HTMLImageElement>('img')?.alt).toBe('New');
    expect(target.querySelector<HTMLImageElement>('img')?.getAttribute('src')).toBe(
      'data:image/png;base64,bmV3'
    );
    expect(target.textContent).not.toContain('旧图片不存在');
  });

  test('binds a pending load to its document path and ignores it after the path changes', async () => {
    const staleLoad = deferred<{ objectUrl: string; path: string }>();
    mocks.loadLocalImage
      .mockReturnValueOnce(staleLoad.promise)
      .mockResolvedValueOnce({
        objectUrl: 'data:image/png;base64,bmV3LXBhdGg=',
        path: 'C:\\new-doc\\images\\shared.png'
      });
    target = document.createElement('div');
    document.body.append(target);
    component = mount(PreviewPaneHarness, {
      target,
      props: {
        html: '<img src="marklite:images%2Fshared%2Epng" alt="Shared">' as SanitizedMarkdownHtml,
        documentPath: 'C:\\old-doc\\index.md',
        alternateDocumentPath: 'C:\\new-doc\\index.md',
        initialAllowLocalImages: true
      }
    });

    await vi.waitFor(() => expect(mocks.loadLocalImage).toHaveBeenCalledTimes(1));
    target
      .querySelector<HTMLButtonElement>('[data-testid="switch-document"]')!
      .dispatchEvent(new MouseEvent('click', { bubbles: true }));
    await tick();
    expect(mocks.loadLocalImage).toHaveBeenCalledTimes(1);
    staleLoad.resolve({
      objectUrl: 'data:image/png;base64,b2xkLXBhdGg=',
      path: 'C:\\old-doc\\images\\shared.png'
    });
    await vi.waitFor(() => expect(mocks.loadLocalImage).toHaveBeenCalledTimes(2));
    await vi.waitFor(() =>
      expect(target.querySelector<HTMLImageElement>('img')?.getAttribute('src')).toBe(
        'data:image/png;base64,bmV3LXBhdGg='
      )
    );

    expect(mocks.loadLocalImage.mock.calls.map(([path]) => path)).toEqual([
      'C:\\old-doc\\index.md',
      'C:\\new-doc\\index.md'
    ]);
    expect(target.querySelector<HTMLImageElement>('img')?.getAttribute('src')).toBe(
      'data:image/png;base64,bmV3LXBhdGg='
    );
  });

  test('disabling images performs zero additional reads and enabling retries from immutable HTML', async () => {
    mocks.loadLocalImage.mockResolvedValue({
      objectUrl: 'data:image/png;base64,cG5n',
      path: 'C:\\docs\\images\\pixel.png'
    });
    target = document.createElement('div');
    document.body.append(target);
    component = mount(PreviewPaneHarness, {
      target,
      props: {
        html: '<img src="marklite:images%2Fpixel%2Epng" alt="Pixel">' as SanitizedMarkdownHtml,
        documentPath: 'C:\\docs\\index.md',
        initialAllowLocalImages: true
      }
    });

    await vi.waitFor(() => expect(target.querySelector('img')).not.toBeNull());
    expect(mocks.loadLocalImage).toHaveBeenCalledTimes(1);
    target
      .querySelector<HTMLButtonElement>('[data-testid="disable-images"]')!
      .dispatchEvent(new MouseEvent('click', { bubbles: true }));

    await vi.waitFor(() =>
      expect(target.querySelector('.local-image-placeholder')?.textContent).toContain(
        'Local images are disabled'
      )
    );
    expect(mocks.loadLocalImage).toHaveBeenCalledTimes(1);

    target
      .querySelector<HTMLButtonElement>('[data-testid="enable-images"]')!
      .dispatchEvent(new MouseEvent('click', { bubbles: true }));
    await vi.waitFor(() => expect(target.querySelector('img')).not.toBeNull());
    expect(mocks.loadLocalImage).toHaveBeenCalledTimes(2);
  });

  test('loads one resource for repeated image targets in the same preview job', async () => {
    mocks.loadLocalImage.mockResolvedValue({
      objectUrl: 'blob:shared-image',
      path: 'C:\\docs\\images\\shared.png'
    });
    target = document.createElement('div');
    document.body.append(target);
    component = mount(PreviewPaneHarness, {
      target,
      props: {
        html: [
          '<img src="marklite:images%2Fshared%2Epng" alt="First">',
          '<img src="marklite:images%2Fshared%2Epng" alt="Second">'
        ].join('') as SanitizedMarkdownHtml,
        documentPath: 'C:\\docs\\index.md',
        initialAllowLocalImages: true
      }
    });

    await vi.waitFor(() => expect(target.querySelectorAll('img')).toHaveLength(2));
    expect(mocks.loadLocalImage).toHaveBeenCalledOnce();
    expect(
      [...target.querySelectorAll<HTMLImageElement>('img')].map((image) => image.src)
    ).toEqual(['blob:shared-image', 'blob:shared-image']);
  });

  test('maps 64 source aliases to one canonical resource and one object URL', async () => {
    const sources = Array.from({ length: 64 }, (_, index) => `marklite:images%2Falias-${index}%2Epng`);
    mocks.loadLocalImages.mockResolvedValueOnce({
      entries: sources.map((source) => ({
        target: source,
        resource: {
          objectUrl: 'blob:canonical-image',
          path: 'C:\\docs\\images\\canonical.png',
          width: 32,
          height: 16,
          encodedBytes: 128,
          decodedBytes: 2048
        },
        error: null
      })),
      objectUrls: ['blob:canonical-image']
    });
    await render(
      sources.map((source, index) => `<img src="${source}" alt="Alias ${index}">`).join(''),
      { allowLocalImages: true }
    );

    await vi.waitFor(() => expect(target.querySelectorAll('img')).toHaveLength(64));
    expect(mocks.loadLocalImages).toHaveBeenCalledOnce();
    expect(mocks.loadLocalImages.mock.calls[0][1]).toEqual(sources);
    expect(new Set([...target.querySelectorAll<HTMLImageElement>('img')].map((image) => image.src))).toEqual(
      new Set(['blob:canonical-image'])
    );
    expect(target.querySelector('img')?.getAttribute('loading')).toBe('lazy');
    expect(target.querySelector('img')?.getAttribute('decoding')).toBe('async');
  });

  test('commits a mixed multi-image result only after every current image settles', async () => {
    const slowImage = deferred<{ objectUrl: string; path: string }>();
    mocks.loadLocalImage.mockImplementation((_documentPath: string | null, source: string) => {
      if (source.includes('first')) {
        return Promise.resolve({
          objectUrl: 'data:image/png;base64,Zmlyc3Q=',
          path: 'C:\\docs\\images\\first.png'
        });
      }
      if (source.includes('slow')) return slowImage.promise;
      return Promise.reject({ code: 'FILE_NOT_FOUND', message: '图片不存在：missing.png' });
    });
    await render(
      [
        '<img src="marklite:images%2Ffirst%2Epng" alt="First">',
        '<img src="marklite:images%2Fslow%2Epng" alt="Slow">',
        '<img src="marklite:images%2Fmissing%2Epng" alt="Missing">'
      ].join(''),
      { allowLocalImages: true }
    );

    await vi.waitFor(() => expect(mocks.loadLocalImage).toHaveBeenCalledTimes(3));
    expect(target.querySelectorAll('img')).toHaveLength(0);
    expect(target.querySelectorAll('.local-image-placeholder')).toHaveLength(3);

    slowImage.resolve({
      objectUrl: 'data:image/png;base64,c2xvdw==',
      path: 'C:\\docs\\images\\slow.png'
    });

    await vi.waitFor(() => expect(target.querySelectorAll('img')).toHaveLength(2));
    expect(
      Array.from(target.querySelectorAll<HTMLImageElement>('img')).map((image) =>
        image.getAttribute('src')
      )
    ).toEqual(['data:image/png;base64,Zmlyc3Q=', 'data:image/png;base64,c2xvdw==']);
    expect(target.querySelectorAll('.local-image-placeholder')).toHaveLength(1);
    expect(target.querySelector('.local-image-placeholder')?.textContent).toContain(
      'The file does not exist.'
    );
  });

  test('coalesces rapid revisions instead of multiplying image-read concurrency', async () => {
    const oldLoads = Array.from({ length: 4 }, () =>
      deferred<{ objectUrl: string; path: string }>()
    );
    oldLoads.forEach((load) => mocks.loadLocalImage.mockReturnValueOnce(load.promise));
    mocks.loadLocalImage.mockResolvedValue({
      objectUrl: 'data:image/png;base64,bGF0ZXN0',
      path: 'C:\\docs\\images\\latest.png'
    });
    target = document.createElement('div');
    document.body.append(target);
    component = mount(PreviewPaneHarness, {
      target,
      props: {
        html: Array.from(
          { length: 4 },
          (_, index) => `<img src="marklite:images%2Fold-${index}%2Epng" alt="Old ${index}">`
        ).join('') as SanitizedMarkdownHtml,
        alternateHtml: '<img src="marklite:images%2Flatest%2Epng" alt="Latest">' as SanitizedMarkdownHtml,
        documentPath: 'C:\\docs\\index.md',
        initialAllowLocalImages: true
      }
    });

    await vi.waitFor(() => expect(mocks.loadLocalImage).toHaveBeenCalledTimes(4));
    const switchButton = target.querySelector<HTMLButtonElement>('[data-testid="switch-html"]')!;
    for (let index = 0; index < 19; index += 1) switchButton.click();
    await tick();
    expect(mocks.loadLocalImage).toHaveBeenCalledTimes(4);
    const cancelledJobId = mocks.loadLocalImage.mock.calls[0][2];
    expect(mocks.cancelLocalImageJob).toHaveBeenCalledWith(cancelledJobId);

    oldLoads.forEach((load, index) =>
      load.resolve({
        objectUrl: `data:image/png;base64,b2xk${index}`,
        path: `C:\\docs\\images\\old-${index}.png`
      })
    );
    await vi.waitFor(() => expect(mocks.loadLocalImage).toHaveBeenCalledTimes(5));
    await vi.waitFor(() => expect(target.querySelector<HTMLImageElement>('img')?.alt).toBe('Latest'));
    expect(target.querySelectorAll('img')).toHaveLength(1);
  });

  test('revokes object URLs when a preview revision is replaced and when it is destroyed', async () => {
    const originalDescriptor = Object.getOwnPropertyDescriptor(URL, 'revokeObjectURL');
    const revokeObjectUrl = vi.fn();
    Object.defineProperty(URL, 'revokeObjectURL', {
      configurable: true,
      value: revokeObjectUrl
    });
    try {
      mocks.loadLocalImage
        .mockResolvedValueOnce({ objectUrl: 'blob:old-image', path: 'C:\\docs\\old.png' })
        .mockResolvedValueOnce({ objectUrl: 'blob:new-image', path: 'C:\\docs\\new.png' });
      target = document.createElement('div');
      document.body.append(target);
      component = mount(PreviewPaneHarness, {
        target,
        props: {
          html: '<img src="marklite:old%2Epng" alt="Old">' as SanitizedMarkdownHtml,
          alternateHtml: '<img src="marklite:new%2Epng" alt="New">' as SanitizedMarkdownHtml,
          documentPath: 'C:\\docs\\index.md',
          initialAllowLocalImages: true
        }
      });

      await vi.waitFor(() =>
        expect(target.querySelector<HTMLImageElement>('img')?.getAttribute('src')).toBe(
          'blob:old-image'
        )
      );
      target.querySelector<HTMLButtonElement>('[data-testid="switch-html"]')!.click();
      await vi.waitFor(() =>
        expect(target.querySelector<HTMLImageElement>('img')?.getAttribute('src')).toBe(
          'blob:new-image'
        )
      );
      expect(revokeObjectUrl).toHaveBeenCalledWith('blob:old-image');

      await unmount(component);
      component = undefined;
      expect(revokeObjectUrl).toHaveBeenCalledWith('blob:new-image');
    } finally {
      if (originalDescriptor) {
        Object.defineProperty(URL, 'revokeObjectURL', originalDescriptor);
      } else {
        Reflect.deleteProperty(URL, 'revokeObjectURL');
      }
    }
  });

  test('does not write the detached result after the component is destroyed', async () => {
    const pending = deferred<{ objectUrl: string; path: string }>();
    mocks.loadLocalImage.mockReturnValue(pending.promise);
    await render('<img src="marklite:images%2Fslow%2Epng" alt="Slow">', {
      allowLocalImages: true
    });
    await vi.waitFor(() => expect(mocks.loadLocalImage).toHaveBeenCalledTimes(1));

    await unmount(component!);
    component = undefined;
    pending.resolve({
      objectUrl: 'data:image/png;base64,c2xvdw==',
      path: 'C:\\docs\\images\\slow.png'
    });
    await tick();

    expect(target.querySelector('.preview')).toBeNull();
    expect(target.querySelector('img')).toBeNull();
  });
});

describe('PreviewPane Mermaid lifecycle', () => {
  test('commits a validated diagram with accessible horizontal overflow and preview typography', async () => {
    const source = diagramSource(0);
    mocks.renderDiagrams.mockResolvedValue({ artifacts: [renderedDiagram(source, 'Release flow')], diagnostics: [] });
    target = document.createElement('div');
    document.body.append(target);
    component = mount(PreviewPane, {
      target,
      props: {
        html: '<pre><code class="language-mermaid">flowchart TD\nA--&gt;B</code></pre>' as SanitizedMarkdownHtml,
        tabId: 'diagram-tab', contentRevision: 4, renderedRevision: 4, sourceBlocks: [],
        diagrams: [source], diagramDiagnostics: [],
        settings: settings({ previewFontFamily: 'Inter', previewFontSize: 18, lineHeight: 1.7 }),
        documentPath: null, documentTitle: 'Diagram.md', outline: [],
        onOpenDocument: vi.fn(async () => true), onJumpToLine: vi.fn()
      }
    });

    await vi.waitFor(() => expect(target.querySelector('.mermaid-diagram')).not.toBeNull());
    const figure = target.querySelector<HTMLElement>('.mermaid-diagram')!;
    expect(figure.getAttribute('aria-label')).toBe('Release flow');
    expect(figure.querySelector('svg')?.getAttribute('role')).toBe('img');
    expect(figure.textContent).toContain('Release flow description');
    expect(target.querySelector('code.language-mermaid')).toBeNull();
    expect(getComputedStyle(figure).overflowX).toBe('auto');
    expect(mocks.renderDiagrams).toHaveBeenCalledWith(
      [source], [], 'light', { fontFamily: 'Inter', fontSize: 18, lineHeight: 1.7 }
    );
  });

  test('keeps source positions and order for multiple diagrams', async () => {
    const first = diagramSource(0);
    const second = diagramSource(1);
    mocks.renderDiagrams.mockResolvedValue({
      artifacts: [renderedDiagram(first, 'First'), renderedDiagram(second, 'Second')],
      diagnostics: []
    });
    target = document.createElement('div');
    document.body.append(target);
    component = mount(PreviewPane, {
      target,
      props: {
        html: '\ue000MARKLITE_BLOCK_0\ue001<pre><code class="language-mermaid">flowchart TD\nA--&gt;B</code></pre>\ue000MARKLITE_BLOCK_1\ue001<pre><code class="language-mermaid">flowchart TD\nC--&gt;D</code></pre>' as SanitizedMarkdownHtml,
        tabId: 'multi-tab', contentRevision: 1, renderedRevision: 1,
        sourceBlocks: [
          { startUtf16: 0, endUtf16: 20, startLine: 1, endLine: 2 },
          { startUtf16: 21, endUtf16: 40, startLine: 4, endLine: 5 }
        ],
        diagrams: [first, second], diagramDiagnostics: [], settings: settings(),
        documentPath: null, documentTitle: 'Multi.md', outline: [],
        onOpenDocument: vi.fn(async () => true), onJumpToLine: vi.fn()
      }
    });
    await vi.waitFor(() => expect(target.querySelectorAll('.mermaid-diagram')).toHaveLength(2));
    expect(Array.from(target.querySelectorAll('.mermaid-diagram')).map((element) => element.getAttribute('data-marklite-source-block'))).toEqual(['0', '1']);
    expect(Array.from(target.querySelectorAll('.mermaid-diagram')).map((element) => element.getAttribute('aria-label'))).toEqual(['First', 'Second']);
  });

  test('keeps Mermaid source visible with a byte-range diagnostic when rendering fails', async () => {
    const source = diagramSource(0);
    mocks.renderDiagrams.mockResolvedValue({
      artifacts: [],
      diagnostics: [{
        code: 'DIAGRAM_UNSUPPORTED_TYPE', diagramId: source.diagramId,
        sourceStartByte: source.sourceStartByte, sourceEndByte: source.sourceEndByte,
        message: 'unsupported diagram type', retryable: false
      }]
    });
    target = document.createElement('div');
    document.body.append(target);
    component = mount(PreviewPane, {
      target,
      props: {
        html: '<pre><code class="language-mermaid">unknownDiagram</code></pre>' as SanitizedMarkdownHtml,
        diagrams: [source], diagramDiagnostics: [], settings: settings(),
        documentPath: null, documentTitle: 'Broken.md', outline: [],
        onOpenDocument: vi.fn(async () => true), onJumpToLine: vi.fn()
      }
    });

    await vi.waitFor(() => expect(target.querySelector('.mermaid-diagram-error')).not.toBeNull());
    expect(target.querySelector('code.language-mermaid')?.textContent).toContain('unknownDiagram');
    expect(target.querySelector('.mermaid-diagram-error')?.textContent).toContain('unsupported diagram type');
    expect(target.querySelector('.mermaid-diagram-error')?.textContent).toContain(
      `${source.sourceStartByte}–${source.sourceEndByte}`
    );
  });

  test('holds fragment navigation until the current diagram layout settles', async () => {
    const source = diagramSource(0);
    const batch = deferred<{ artifacts: RenderedDiagram[]; diagnostics: [] }>();
    mocks.renderDiagrams.mockReturnValue(batch.promise);
    const toast = vi.spyOn(uiActions, 'toast');
    target = document.createElement('div');
    document.body.append(target);
    component = mount(PreviewPane, {
      target,
      props: {
        html: '<h2 id="later">Later</h2><pre><code class="language-mermaid">flowchart TD\nA--&gt;B</code></pre>' as SanitizedMarkdownHtml,
        tabId: 'layout-tab', contentRevision: 2, renderedRevision: 2,
        diagrams: [source], diagramDiagnostics: [], settings: settings(), documentPath: null,
        documentTitle: 'Layout.md', outline: [],
        onOpenDocument: vi.fn(async () => true), onJumpToLine: vi.fn()
      }
    });
    (component as typeof component & { scrollToFragment(fragment: string): void }).scrollToFragment('later');
    await vi.waitFor(() => expect(target.querySelector('#later')).not.toBeNull());
    expect(toast).not.toHaveBeenCalled();

    batch.resolve({ artifacts: [renderedDiagram(source, 'Settled')], diagnostics: [] });
    await vi.waitFor(() => expect(target.querySelector('.mermaid-diagram')).not.toBeNull());
    expect(toast).not.toHaveBeenCalled();
    toast.mockRestore();
  });

  test('rejects a late diagram batch after the preview revision switches', async () => {
    const oldSource = diagramSource(0, 'Old-->Result');
    const nextSource = diagramSource(1, 'Next-->Result');
    const oldBatch = deferred<{ artifacts: RenderedDiagram[]; diagnostics: [] }>();
    mocks.renderDiagrams
      .mockReturnValueOnce(oldBatch.promise)
      .mockResolvedValueOnce({ artifacts: [renderedDiagram(nextSource, 'Next result')], diagnostics: [] });
    target = document.createElement('div');
    document.body.append(target);
    component = mount(PreviewPaneHarness, {
      target,
      props: {
        html: '<pre><code class="language-mermaid">flowchart TD\nOld--&gt;Result</code></pre>' as SanitizedMarkdownHtml,
        alternateHtml: '<pre><code class="language-mermaid">flowchart TD\nNext--&gt;Result</code></pre>' as SanitizedMarkdownHtml,
        documentPath: 'C:\\docs\\diagram.md', diagrams: [oldSource], alternateDiagrams: [nextSource]
      }
    });
    await vi.waitFor(() => expect(mocks.renderDiagrams).toHaveBeenCalledTimes(1));
    target.querySelector<HTMLButtonElement>('[data-testid="switch-html"]')!.click();
    await tick();
    oldBatch.resolve({ artifacts: [renderedDiagram(oldSource, 'Old result')], diagnostics: [] });

    await vi.waitFor(() => expect(mocks.renderDiagrams).toHaveBeenCalledTimes(2));
    await vi.waitFor(() => expect(target.querySelector('.mermaid-diagram')?.textContent).toContain('Next result'));
    expect(target.textContent).not.toContain('Old result');
    expect(mocks.cancelDiagrams).toHaveBeenCalled();
  });

  test('rerenders the same source when theme or preview typography changes', async () => {
    const source = diagramSource(0);
    mocks.renderDiagrams.mockResolvedValue({ artifacts: [renderedDiagram(source, 'Stable source')], diagnostics: [] });
    target = document.createElement('div');
    document.body.append(target);
    component = mount(PreviewPaneHarness, {
      target,
      props: {
        html: '<pre><code class="language-mermaid">flowchart TD\nA--&gt;B</code></pre>' as SanitizedMarkdownHtml,
        documentPath: 'C:\\docs\\diagram.md', diagrams: [source]
      }
    });
    await vi.waitFor(() => expect(mocks.renderDiagrams).toHaveBeenCalledTimes(1));
    target.querySelector<HTMLButtonElement>('[data-testid="dark-theme"]')!.click();
    await vi.waitFor(() => expect(mocks.renderDiagrams).toHaveBeenCalledTimes(2));
    expect(mocks.renderDiagrams.mock.calls[1][2]).toBe('dark');

    target.querySelector<HTMLButtonElement>('[data-testid="increase-preview-font"]')!.click();
    await vi.waitFor(() => expect(mocks.renderDiagrams).toHaveBeenCalledTimes(3));
    expect(mocks.renderDiagrams.mock.calls[2][3].fontSize).toBe(defaultSettings.previewFontSize + 1);
  });

  test('releases preview-owned diagram resources through 100 mount and destroy cycles', async () => {
    const startReleases = mocks.releaseDiagrams.mock.calls.length;
    for (let index = 0; index < 100; index += 1) {
      const cycleTarget = document.createElement('div');
      document.body.append(cycleTarget);
      const cycle = mount(PreviewPane, {
        target: cycleTarget,
        props: {
          html: '' as SanitizedMarkdownHtml, settings: settings(), documentPath: null,
          documentTitle: 'Cycle.md', outline: [],
          onOpenDocument: vi.fn(async () => true), onJumpToLine: vi.fn()
        }
      });
      await tick();
      await unmount(cycle);
      cycleTarget.remove();
    }
    expect(mocks.releaseDiagrams.mock.calls.length - startReleases).toBe(100);
    expect(document.querySelectorAll('iframe[aria-hidden="true"]')).toHaveLength(0);
  });
});
