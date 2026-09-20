/// <reference types="node" />

import { readFileSync } from 'node:fs';
import { mount, tick, unmount } from 'svelte';
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';
import type { EditorTab } from '../../app/stores/documentStore';
import { EMPTY_SANITIZED_MARKDOWN_HTML } from '../../lib/tauriApi';
import SidebarHarness from '../../test/SidebarHarness.svelte';
import Sidebar from './Sidebar.svelte';

const globalsCss = readFileSync('src/styles/globals.css', 'utf8');

vi.mock('lucide-svelte', async () => {
  const { default: IconStub } = await import('../../test/IconStub.svelte');
  return {
    Clock3: IconStub,
    FileText: IconStub,
    FolderOpen: IconStub,
    Info: IconStub,
    ListTree: IconStub,
    PanelLeftClose: IconStub,
    Trash2: IconStub
  };
});

let target: HTMLDivElement;
let component: ReturnType<typeof mount> | undefined;
let globalStyle: HTMLStyleElement;

const recentFiles = [
  {
    path: 'C:\\docs\\short.md',
    title: 'short.md',
    lastOpenedAt: '2026-08-09T10:00:00Z'
  },
  {
    path: 'C:\\docs\\这是一个特别长而且需要被省略的中文文件名称.md',
    title: '这是一个特别长而且需要被省略的中文文件名称.md',
    lastOpenedAt: '2026-08-09T10:01:00Z'
  },
  {
    path: 'C:\\docs\\averyveryveryveryverylongfilenamewithoutspaces.md',
    title: 'averyveryveryveryverylongfilenamewithoutspaces.md',
    lastOpenedAt: '2026-08-09T10:02:00Z'
  }
];

function sidebarCallbacks(overrides: Record<string, unknown> = {}) {
  return {
    onTabChange: vi.fn(),
    onOpenRecent: vi.fn(),
    onRemoveRecent: vi.fn(),
    onRevealRecent: vi.fn(),
    onJumpToLine: vi.fn(),
    onCollapse: vi.fn(),
    ...overrides
  };
}

async function renderSidebar(onRemoveRecent = vi.fn()) {
  target = document.createElement('div');
  target.style.width = '180px';
  document.body.append(target);
  component = mount(Sidebar, {
    target,
    props: {
      activeSidebarTab: 'recent',
      recentFiles,
      tab: undefined,
      ...sidebarCallbacks({ onRemoveRecent })
    }
  });
  await tick();
  return onRemoveRecent;
}

function editorTab(path: string | null, id: string): EditorTab {
  return {
    id,
    path,
    fileIdentity: path ? `test-file:${path}` : null,
    contentVersion: path ? `sha256:${path}` : null,
    title: path?.split(/[\\/]/).at(-1) ?? 'Untitled.md',
    content: '',
    contentRevision: 0,
    isDirty: false,
    isWelcome: false,
    loadState: 'loaded',
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

async function renderCurrentHarness() {
  target = document.createElement('div');
  document.body.append(target);
  component = mount(SidebarHarness, {
    target,
    props: {
      recentFiles: recentFiles.slice(0, 2),
      firstTab: { ...editorTab(recentFiles[0].path, 'first'), isDirty: true },
      secondTab: editorTab(recentFiles[1].path, 'second'),
      unsavedTab: editorTab(null, 'unsaved')
    }
  });
  await tick();
}

beforeEach(() => {
  globalStyle = document.createElement('style');
  globalStyle.textContent = globalsCss;
  document.head.append(globalStyle);
});

afterEach(async () => {
  if (component) await unmount(component);
  component = undefined;
  target?.remove();
  globalStyle.remove();
  vi.restoreAllMocks();
});

describe('Sidebar recent file icon sizing', () => {
  test('exposes an explicit internal collapse action', async () => {
    const onCollapse = vi.fn();
    target = document.createElement('div');
    document.body.append(target);
    component = mount(Sidebar, {
      target,
      props: {
        activeSidebarTab: 'recent',
        recentFiles,
        tab: undefined,
        ...sidebarCallbacks({ onCollapse })
      }
    });
    await tick();

    target.querySelector<HTMLButtonElement>('[aria-label="Collapse sidebar"]')!.click();
    expect(onCollapse).toHaveBeenCalledOnce();
  });

  test('keeps every leading file icon at a fixed 15px flex size in a narrow container', async () => {
    await renderSidebar();

    const icons = [...target.querySelectorAll<SVGElement>('.recent-file-icon')];
    expect(icons).toHaveLength(recentFiles.length);
    for (const icon of icons) {
      expect(icon.getAttribute('width')).toBe('15');
      expect(icon.getAttribute('height')).toBe('15');
    }
    for (const icon of icons) {
      const style = getComputedStyle(icon);
      expect(style.width).toBe('15px');
      expect(style.minWidth).toBe('15px');
      expect(style.maxWidth).toBe('15px');
      expect(style.height).toBe('15px');
      expect(style.flex).toBe('0 0 15px');
    }
  });

  test('keeps text ellipsis rules and each delete icon/click target stable', async () => {
    const onRemoveRecent = await renderSidebar();

    const textContainers = [...target.querySelectorAll<HTMLElement>('.recent-file-text')];
    expect(textContainers).toHaveLength(recentFiles.length);
    for (const container of textContainers) {
      const title = container.querySelector<HTMLElement>('strong')!;
      expect(title.textContent).toBeTruthy();
    }
    for (const container of textContainers) {
      expect(getComputedStyle(container).minWidth).toBe('0px');
      const title = container.querySelector<HTMLElement>('strong')!;
      const style = getComputedStyle(title);
      expect(style.overflow).toBe('hidden');
      expect(style.textOverflow).toBe('ellipsis');
      expect(style.whiteSpace).toBe('nowrap');
    }

    const buttons = [...target.querySelectorAll<HTMLButtonElement>('.icon-danger')];
    const icons = [...target.querySelectorAll<SVGElement>('.recent-action-icon')];
    expect(buttons).toHaveLength(recentFiles.length);
    expect(icons).toHaveLength(recentFiles.length);
    for (const button of buttons) {
      expect(button.type).toBe('button');
    }
    for (const icon of icons) {
      expect(icon.getAttribute('width')).toBe('14');
      expect(icon.getAttribute('height')).toBe('14');
    }
    for (const button of buttons) {
      const style = getComputedStyle(button);
      expect(style.width).toBe('30px');
      expect(style.height).toBe('30px');
    }
    for (const icon of icons) {
      const style = getComputedStyle(icon);
      expect(style.width).toBe('14px');
      expect(style.height).toBe('14px');
      expect(style.flex).toBe('0 0 14px');
    }

    buttons[1].click();
    expect(onRemoveRecent).toHaveBeenCalledWith(recentFiles[1].path);
  });
});

describe('Sidebar current recent file', () => {
  test('reacts to tab switches, unsaved tabs and removal without recreating the recent list', async () => {
    await renderCurrentHarness();

    const currentTitle = () =>
      target.querySelector('.recent-item.current .recent-file-text strong')?.textContent;
    const currentButtons = () =>
      [...target.querySelectorAll<HTMLButtonElement>('.recent-main[aria-current="page"]')];

    expect(currentTitle()).toBe(recentFiles[0].title);
    expect(currentButtons()).toHaveLength(1);

    target.querySelector<HTMLButtonElement>('[data-testid="second-tab"]')!.click();
    await tick();
    expect(currentTitle()).toBe(recentFiles[1].title);
    expect(currentButtons()).toHaveLength(1);

    target.querySelector<HTMLButtonElement>('[data-testid="unsaved-tab"]')!.click();
    await tick();
    expect(currentTitle()).toBeUndefined();
    expect(currentButtons()).toHaveLength(0);

    target.querySelector<HTMLButtonElement>('[data-testid="first-tab"]')!.click();
    await tick();
    target.querySelector<HTMLButtonElement>('.recent-item.current .icon-danger')!.click();
    await tick();
    expect(currentTitle()).toBeUndefined();
    expect(currentButtons()).toHaveLength(0);
  });

  test('marks only the first exact match when duplicate recent entries are present', async () => {
    const aliases = [
      recentFiles[0],
      { ...recentFiles[0], title: 'duplicate-entry.md' }
    ];
    target = document.createElement('div');
    document.body.append(target);
    component = mount(Sidebar, {
      target,
      props: {
        activeSidebarTab: 'recent',
        recentFiles: aliases,
        tab: editorTab('C:\\docs\\short.md', 'active'),
        ...sidebarCallbacks()
      }
    });
    await tick();

    expect(target.querySelectorAll('.recent-item.current')).toHaveLength(1);
    expect(target.querySelectorAll('.recent-main[aria-current="page"]')).toHaveLength(1);
    expect(target.querySelector('.recent-item.current strong')?.textContent).toBe('short.md');
  });

  test('matches the backend canonical POSIX path without collapsing case-distinct files', async () => {
    const posixRecent = [
      { ...recentFiles[0], path: '/home/vazone/docs/Note.md', title: 'Note.md' },
      { ...recentFiles[1], path: '/home/vazone/docs/note.md', title: 'note.md' }
    ];
    target = document.createElement('div');
    document.body.append(target);
    component = mount(Sidebar, {
      target,
      props: {
        activeSidebarTab: 'recent',
        recentFiles: posixRecent,
        tab: editorTab('/home/vazone/docs/Note.md', 'active-posix'),
        ...sidebarCallbacks()
      }
    });
    await tick();

    expect(target.querySelectorAll('.recent-item.current')).toHaveLength(1);
    expect(target.querySelector('.recent-item.current strong')?.textContent).toBe('Note.md');
  });

  test('keeps current, hover and keyboard focus states structurally distinct', async () => {
    await renderCurrentHarness();

    const current = target.querySelector<HTMLElement>('.recent-item.current')!;
    const currentMain = current.querySelector<HTMLButtonElement>('.recent-main')!;
    const otherMain = target.querySelector<HTMLButtonElement>('.recent-item:not(.current) .recent-main')!;

    expect(currentMain.getAttribute('aria-current')).toBe('page');
    expect(otherMain.hasAttribute('aria-current')).toBe(false);
    otherMain.focus();
    expect(document.activeElement).toBe(otherMain);
  });
});

describe('Sidebar outline and document information', () => {
  test('selects the current heading from an ordered outline and jumps to it', async () => {
    const onJumpToLine = vi.fn();
    const tab = editorTab('C:\\docs\\outline.md', 'outline');
    tab.scrollPosition.line = 18;
    tab.outline = [
      { level: 1, title: 'Start', line: 1, slug: 'start' },
      { level: 2, title: 'Current', line: 12, slug: 'current' },
      { level: 2, title: 'Later', line: 30, slug: 'later' }
    ];
    target = document.createElement('div');
    document.body.append(target);
    component = mount(Sidebar, {
      target,
      props: {
        activeSidebarTab: 'outline',
        recentFiles: [],
        tab,
        ...sidebarCallbacks({ onJumpToLine })
      }
    });
    await tick();

    expect(target.querySelector('.outline-item.current')?.textContent).toContain('Current');
    target.querySelector<HTMLButtonElement>('.outline-item.current')!.click();
    expect(onJumpToLine).toHaveBeenCalledWith(12);
  });

  test.each([
    [null, '-'],
    [0, '0.0 KB'],
    [1536, '1.5 KB']
  ])('renders file size %s as %s', async (fileSize, expected) => {
    const tab = editorTab('C:\\docs\\size.md', `size-${fileSize}`);
    tab.fileSize = fileSize;
    target = document.createElement('div');
    document.body.append(target);
    component = mount(Sidebar, {
      target,
      props: {
        activeSidebarTab: 'info',
        recentFiles: [],
        tab,
        ...sidebarCallbacks()
      }
    });
    await tick();

    const sizeRow = [...target.querySelectorAll<HTMLElement>('.info-list > div')]
      .find((row) => row.querySelector('dt')?.textContent === 'Size');
    expect(sizeRow?.querySelector('dd')?.textContent).toBe(expected);
  });
});

describe('Sidebar context menu positioning', () => {
  test('shrinks and clamps the menu inside all four edges of a tiny viewport', async () => {
    vi.spyOn(window, 'innerWidth', 'get').mockReturnValue(160);
    vi.spyOn(window, 'innerHeight', 'get').mockReturnValue(120);
    await renderSidebar();

    target.querySelector<HTMLElement>('.recent-item')!.dispatchEvent(
      new MouseEvent('contextmenu', {
        bubbles: true,
        cancelable: true,
        clientX: 159,
        clientY: 119
      })
    );
    await tick();

    const menu = target.querySelector<HTMLElement>('.sidebar-context-menu')!;
    const left = Number.parseFloat(menu.style.left);
    const top = Number.parseFloat(menu.style.top);
    const width = Number.parseFloat(menu.style.width);
    const maxHeight = Number.parseFloat(menu.style.maxHeight);
    expect(left).toBeGreaterThanOrEqual(8);
    expect(top).toBeGreaterThanOrEqual(8);
    expect(left + width).toBeLessThanOrEqual(152);
    expect(top + maxHeight).toBeLessThanOrEqual(112);
  });
});
