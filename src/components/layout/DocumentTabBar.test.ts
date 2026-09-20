import { mount, tick, unmount } from 'svelte';
import { afterEach, describe, expect, test, vi } from 'vitest';
import type { EditorTab } from '../../app/stores/documentStore';
import { EMPTY_SANITIZED_MARKDOWN_HTML } from '../../lib/tauriApi';
import DocumentTabBar from './DocumentTabBar.svelte';

let target: HTMLDivElement;
let component: ReturnType<typeof mount> | undefined;
let originalScrollIntoView: typeof HTMLElement.prototype.scrollIntoView | undefined;
let scrollIntoView: ReturnType<typeof vi.fn>;

function tab(id: string, title: string, isDirty = false): EditorTab {
  return {
    id,
    path: `C:\\docs\\${title}`,
    fileIdentity: `test-file:${title}`,
    contentVersion: `sha256:${title}`,
    title,
    content: '',
    contentRevision: 0,
    isDirty,
    isWelcome: false,
    loadState: id === 'third' ? 'unloaded' : 'loaded',
    loadError: null,
    lastSavedAt: null,
    fileSize: null,
    cursorPosition: { line: 1, column: 1 },
    scrollPosition: {
      line: 1,
      ratio: 0,
      totalLines: 1,
      scrollTop: 0,
      scrollHeight: 0,
      clientHeight: 0
    },
    editorState: null,
    html: EMPTY_SANITIZED_MARKDOWN_HTML,
    sourceBlocks: [],
    virtualPreview: null,
    diagrams: [],
    diagramDiagnostics: [],
    renderedRevision: -1,
    analysisRevision: -1,
    outline: [],
    stats: {
      wordCount: 0,
      characterCount: 0,
      lineCount: 1,
      headingCount: 0,
      linkCount: 0,
      imageCount: 0
    }
  };
}

async function renderTabBar(overrides: Record<string, unknown> = {}) {
  const callbacks = {
    onActivate: vi.fn(),
    onClose: vi.fn(),
    onCloseOthers: vi.fn(),
    onCloseRight: vi.fn()
  };
  target = document.createElement('div');
  document.body.append(target);
  component = mount(DocumentTabBar, {
    target,
    props: {
      tabs: [tab('first', 'first.md'), tab('second', 'second.md', true), tab('third', 'third.md')],
      activeTabId: 'first',
      ...callbacks,
      ...overrides
    }
  });
  await tick();
  return callbacks;
}

function openMouseMenu(tabId: string, clientX = 400, clientY = 300) {
  const item = target.querySelector<HTMLElement>(`[data-tab-id="${tabId}"]`)!;
  item.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX, clientY }));
}

afterEach(async () => {
  if (component) await unmount(component);
  component = undefined;
  target?.remove();
  vi.restoreAllMocks();
  if (originalScrollIntoView) {
    Object.defineProperty(HTMLElement.prototype, 'scrollIntoView', {
      configurable: true,
      value: originalScrollIntoView
    });
  } else {
    delete (HTMLElement.prototype as Partial<HTMLElement>).scrollIntoView;
  }
});

function observeScrollIntoView() {
  originalScrollIntoView = HTMLElement.prototype.scrollIntoView;
  scrollIntoView = vi.fn();
  Object.defineProperty(HTMLElement.prototype, 'scrollIntoView', {
    configurable: true,
    value: scrollIntoView
  });
  return scrollIntoView;
}

function setOverflowMetrics(tabbar: HTMLElement, scrollWidth: number, clientWidth: number) {
  Object.defineProperties(tabbar, {
    scrollWidth: { configurable: true, value: scrollWidth },
    clientWidth: { configurable: true, value: clientWidth }
  });
}

describe('DocumentTabBar context menu', () => {
  test('opens on right click and dispatches close-right for the triggering tab', async () => {
    const callbacks = await renderTabBar();

    openMouseMenu('second');
    await tick();

    const menu = target.querySelector<HTMLElement>('[role="menu"]')!;
    expect(menu).toBeTruthy();
    expect(menu.getAttribute('aria-label')).toBe('Tab actions');
    const items = [...menu.querySelectorAll<HTMLButtonElement>('[role="menuitem"]')];
    expect(items.map((item) => item.textContent?.trim())).toEqual([
      'Close tab',
      'Close other tabs',
      'Close tabs to the right'
    ]);
    expect(items[1].disabled).toBe(false);
    expect(items[2].disabled).toBe(false);

    items[2].click();
    expect(callbacks.onCloseRight).toHaveBeenCalledWith('second');
    await tick();
    expect(target.querySelector('[role="menu"]')).toBeNull();
  });

  test('disables actions that have no target', async () => {
    await renderTabBar({ tabs: [tab('only', 'only.md')], activeTabId: 'only' });

    openMouseMenu('only');
    await tick();

    const items = [...target.querySelectorAll<HTMLButtonElement>('[role="menuitem"]')];
    expect(items[0].disabled).toBe(false);
    expect(items[1].disabled).toBe(true);
    expect(items[2].disabled).toBe(true);
  });

  test('opens from Shift+F10 and Escape closes the menu and restores tab focus', async () => {
    await renderTabBar();
    const trigger = target.querySelector<HTMLButtonElement>('[data-tab-id="first"] .tab-main')!;
    trigger.focus();

    trigger.dispatchEvent(new KeyboardEvent('keydown', { key: 'F10', shiftKey: true, bubbles: true }));
    await vi.waitFor(() => {
      expect(target.querySelector('[role="menu"]')).toBeTruthy();
      expect(document.activeElement?.getAttribute('role')).toBe('menuitem');
    });

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await tick();
    expect(target.querySelector('[role="menu"]')).toBeNull();
    expect(document.activeElement).toBe(trigger);
  });

  test('closes on outside pointerdown and viewport changes', async () => {
    await renderTabBar();
    openMouseMenu('first');
    await tick();

    document.body.dispatchEvent(new Event('pointerdown', { bubbles: true }));
    await tick();
    expect(target.querySelector('[role="menu"]')).toBeNull();

    openMouseMenu('first');
    await tick();
    window.dispatchEvent(new Event('resize'));
    await tick();
    expect(target.querySelector('[role="menu"]')).toBeNull();
  });

  test('shrinks and clamps the menu inside a tiny viewport', async () => {
    vi.spyOn(window, 'innerWidth', 'get').mockReturnValue(160);
    vi.spyOn(window, 'innerHeight', 'get').mockReturnValue(120);
    await renderTabBar();

    openMouseMenu('first', 159, 119);
    await tick();

    const menu = target.querySelector<HTMLElement>('[role="menu"]')!;
    const left = Number.parseFloat(menu.style.left);
    const top = Number.parseFloat(menu.style.top);
    const width = Number.parseFloat(menu.style.width);
    const maxHeight = Number.parseFloat(menu.style.maxHeight);
    expect(left).toBeGreaterThanOrEqual(8);
    expect(top).toBeGreaterThanOrEqual(8);
    expect(left + width).toBeLessThanOrEqual(152);
    expect(top + maxHeight).toBeLessThanOrEqual(112);
  });

  test('keeps activation and the dedicated close button independent', async () => {
    const callbacks = await renderTabBar();
    target.querySelector<HTMLButtonElement>('[data-tab-id="second"] .tab-main')!.click();
    target.querySelector<HTMLButtonElement>('[data-tab-id="second"] .tab-close')!.click();

    expect(callbacks.onActivate).toHaveBeenCalledWith('second');
    expect(callbacks.onClose).toHaveBeenCalledWith('second');
  });
});

describe('DocumentTabBar overflow navigation', () => {
  test('moves and activates tabs with arrows and Home/End, retaining focus if a close is cancelled', async () => {
    const tabs = [tab('first', 'first.md'), tab('second', 'second.md'), tab('third', 'third.md')];
    const callbacks = await renderTabBar({ tabs });
    const first = target.querySelector<HTMLButtonElement>('[data-tab-id="first"] .tab-main')!;
    first.focus();
    first.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowLeft', bubbles: true, cancelable: true }));
    await tick();
    expect(callbacks.onActivate).toHaveBeenLastCalledWith('third');
    expect(document.activeElement?.closest('.document-tab')?.getAttribute('data-tab-id')).toBe('third');

    const third = target.querySelector<HTMLButtonElement>('[data-tab-id="third"] .tab-main')!;
    third.dispatchEvent(new KeyboardEvent('keydown', { key: 'Home', bubbles: true, cancelable: true }));
    await tick();
    expect(callbacks.onActivate).toHaveBeenLastCalledWith('first');
    first.dispatchEvent(new KeyboardEvent('keydown', { key: 'End', bubbles: true, cancelable: true }));
    await tick();
    expect(callbacks.onActivate).toHaveBeenLastCalledWith('third');

    third.dispatchEvent(new KeyboardEvent('keydown', { key: 'Delete', bubbles: true, cancelable: true }));
    await vi.waitFor(() => expect(callbacks.onClose).toHaveBeenCalledWith('third'));
    await tick();
    expect(document.activeElement?.closest('.document-tab')?.getAttribute('data-tab-id')).toBe('third');
  });

  test('uses stable ellipsis markup instead of perpetual title animation for long names', async () => {
    await renderTabBar({ tabs: [tab('long', '这是一个非常非常长的文档标题用于验证省略显示.md')], activeTabId: 'long' });
    const title = target.querySelector<HTMLElement>('.tab-title')!;

    expect(title.classList.contains('long-title')).toBe(false);
    expect(title.textContent).toContain('这是一个非常非常长的文档标题');
  });

  test('reveals the active tab using nearest scrolling', async () => {
    const reveal = observeScrollIntoView();
    await renderTabBar({ activeTabId: 'third' });

    await vi.waitFor(() => expect(reveal).toHaveBeenCalledWith({ block: 'nearest', inline: 'nearest' }));
    expect((reveal.mock.instances[0] as HTMLElement).dataset.tabId).toBe('third');
  });

  test('maps a vertical wheel delta to horizontal movement only while the tabbar can consume it', async () => {
    await renderTabBar();
    const tabbar = target.querySelector<HTMLElement>('.tabbar')!;
    setOverflowMetrics(tabbar, 600, 200);

    const move = new WheelEvent('wheel', { bubbles: true, cancelable: true, deltaY: 120 });
    tabbar.dispatchEvent(move);
    expect(tabbar.scrollLeft).toBe(120);
    expect(move.defaultPrevented).toBe(true);

    tabbar.scrollLeft = 0;
    const horizontal = new WheelEvent('wheel', {
      bubbles: true,
      cancelable: true,
      deltaX: 75,
      deltaY: 10
    });
    tabbar.dispatchEvent(horizontal);
    expect(tabbar.scrollLeft).toBe(75);
    expect(horizontal.defaultPrevented).toBe(true);

    tabbar.scrollLeft = 400;
    const boundary = new WheelEvent('wheel', { bubbles: true, cancelable: true, deltaY: 120 });
    tabbar.dispatchEvent(boundary);
    expect(tabbar.scrollLeft).toBe(400);
    expect(boundary.defaultPrevented).toBe(false);
  });
});
