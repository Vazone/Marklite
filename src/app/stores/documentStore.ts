import { derived, get, writable } from 'svelte/store';
import type { DocumentDto, DocumentStats, OutlineItem, RenderedMarkdownDto } from '../../lib/tauriApi';

export type CursorPosition = {
  line: number;
  column: number;
};

export type EditorScrollPosition = {
  line: number;
  ratio: number;
  totalLines: number;
  scrollTop: number;
  scrollHeight: number;
  clientHeight: number;
};

export type EditorTab = {
  id: string;
  path: string | null;
  title: string;
  content: string;
  isDirty: boolean;
  isWelcome: boolean;
  lastSavedAt: string | null;
  fileSize: number | null;
  cursorPosition: CursorPosition;
  scrollPosition: EditorScrollPosition;
  html: string;
  outline: OutlineItem[];
  stats: DocumentStats;
};

export type DocumentState = {
  tabs: EditorTab[];
  activeTabId: string | null;
};

const welcomeContent = `# 欢迎使用 MarkLite

一个使用 Rust、Tauri、Svelte 和 CodeMirror 构建的轻量 Markdown 编辑器。

- 作者：Vazone
- GitHub：https://github.com/Vazone/Marklite
- 支持实时预览
- 支持多标签和最近文件
- 支持设置、工具栏和常用快捷键

> 新建、打开和保存都在顶部工具栏里。`;

const emptyStats: DocumentStats = {
  wordCount: 0,
  characterCount: 0,
  lineCount: 1,
  headingCount: 0,
  linkCount: 0,
  imageCount: 0
};

function createId(): string {
  return crypto.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

type CreateTabInput = Partial<DocumentDto> & {
  isWelcome?: boolean;
};

function createTab(document: CreateTabInput = {}): EditorTab {
  const content = document.content ?? '';

  return {
    id: createId(),
    path: document.path ?? null,
    title: document.title ?? 'Untitled.md',
    content,
    isDirty: document.isDirty ?? false,
    isWelcome: document.isWelcome ?? false,
    lastSavedAt: document.lastSavedAt ?? null,
    fileSize: document.fileSize ?? null,
    cursorPosition: { line: 1, column: 1 },
    scrollPosition: { line: 1, ratio: 0, totalLines: 1, scrollTop: 0, scrollHeight: 0, clientHeight: 0 },
    html: '',
    outline: [],
    stats: {
      ...emptyStats,
      characterCount: content.length,
      lineCount: content ? content.split('\n').length : 1
    }
  };
}

function createWelcomeTab(): EditorTab {
  return createTab({
    title: 'Untitled.md',
    content: welcomeContent,
    isDirty: false,
    isWelcome: true
  });
}

function createBlankTab(title = 'Untitled.md'): EditorTab {
  return createTab({
    title,
    content: '',
    isDirty: false,
    isWelcome: false
  });
}

function canReplacePlaceholder(tab: EditorTab | undefined): boolean {
  if (!tab) return false;
  return !tab.path && !tab.isDirty && (tab.isWelcome || tab.content.trim().length === 0);
}

function createDocumentStore() {
  const store = writable<DocumentState>({
    tabs: [createWelcomeTab()],
    activeTabId: null
  });

  store.update((state) => ({
    ...state,
    activeTabId: state.tabs[0].id
  }));

  return {
    subscribe: store.subscribe,
    newDocument() {
      store.update((state) => {
        const replacingPlaceholder = state.tabs.length === 1 && canReplacePlaceholder(state.tabs[0]);
        const tab = createBlankTab(replacingPlaceholder ? 'Untitled.md' : `Untitled-${state.tabs.length + 1}.md`);
        return {
          tabs: replacingPlaceholder ? [tab] : [...state.tabs, tab],
          activeTabId: tab.id
        };
      });
    },
    openDocument(document: DocumentDto) {
      store.update((state) => {
        const existing = state.tabs.find((tab) => tab.path && tab.path === document.path);
        if (existing) {
          return { ...state, activeTabId: existing.id };
        }

        const tab = createTab(document);
        if (state.tabs.length === 1 && canReplacePlaceholder(state.tabs[0])) {
          return {
            tabs: [tab],
            activeTabId: tab.id
          };
        }

        return {
          tabs: [...state.tabs, tab],
          activeTabId: tab.id
        };
      });
    },
    setActive(id: string) {
      store.update((state) => ({ ...state, activeTabId: id }));
    },
    updateActiveContent(content: string) {
      store.update((state) => ({
        ...state,
        tabs: state.tabs.map((tab) =>
          tab.id === state.activeTabId
            ? {
                ...tab,
                content,
                isDirty: true,
                isWelcome: false,
                stats: {
                  ...tab.stats,
                  characterCount: content.length,
                  lineCount: content ? content.split('\n').length : 1
                }
              }
            : tab
        )
      }));
    },
    updateActiveCursor(cursorPosition: CursorPosition) {
      store.update((state) => ({
        ...state,
        tabs: state.tabs.map((tab) => (tab.id === state.activeTabId ? { ...tab, cursorPosition } : tab))
      }));
    },
    updateActiveScroll(scrollPosition: EditorScrollPosition) {
      store.update((state) => ({
        ...state,
        tabs: state.tabs.map((tab) => (tab.id === state.activeTabId ? { ...tab, scrollPosition } : tab))
      }));
    },
    updateRendered(tabId: string, rendered: RenderedMarkdownDto) {
      store.update((state) => ({
        ...state,
        tabs: state.tabs.map((tab) =>
          tab.id === tabId
            ? {
                ...tab,
                html: rendered.html,
                outline: rendered.outline,
                stats: rendered.stats
              }
            : tab
        )
      }));
    },
    markSaved(document: DocumentDto) {
      store.update((state) => ({
        ...state,
        tabs: state.tabs.map((tab) =>
          tab.id === state.activeTabId
            ? {
                ...tab,
                path: document.path,
                title: document.title,
                content: document.content,
                isDirty: false,
                isWelcome: false,
                lastSavedAt: document.lastSavedAt,
                fileSize: document.fileSize
              }
            : tab
        )
      }));
    },
    closeTab(id: string) {
      store.update((state) => {
        const nextTabs = state.tabs.filter((tab) => tab.id !== id);
        if (!nextTabs.length) {
          const tab = createBlankTab();
          return {
            tabs: [tab],
            activeTabId: tab.id
          };
        }

        const activeStillExists = nextTabs.some((tab) => tab.id === state.activeTabId);
        const activeTabId = activeStillExists ? state.activeTabId : nextTabs.at(-1)?.id ?? nextTabs[0].id;

        return {
          tabs: nextTabs,
          activeTabId
        };
      });

      const current = get(store);
      if (!current.activeTabId && current.tabs[0]) {
        store.update((state) => ({ ...state, activeTabId: state.tabs[0].id }));
      }
    }
  };
}

export const documentStore = createDocumentStore();

export const activeTab = derived(documentStore, ($documentStore) =>
  $documentStore.tabs.find((tab) => tab.id === $documentStore.activeTabId) ?? $documentStore.tabs[0]
);

export function getActiveTab(): EditorTab {
  return get(activeTab);
}
