import { derived, get, writable } from 'svelte/store';
import type { DocumentDto, DocumentStats, OutlineItem, RenderedMarkdownDto } from '../../lib/tauriApi';
import type { SerializedEditorState } from '../../lib/editorSession';
import { isSameFilePath } from '../../lib/filePathIdentity';

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

export type DocumentLoadState = 'loaded' | 'unloaded' | 'loading' | 'error';

export type EditorTab = {
  id: string;
  path: string | null;
  title: string;
  content: string;
  contentRevision: number;
  isDirty: boolean;
  isWelcome: boolean;
  loadState: DocumentLoadState;
  loadError: string | null;
  lastSavedAt: string | null;
  fileSize: number | null;
  cursorPosition: CursorPosition;
  scrollPosition: EditorScrollPosition;
  editorState: SerializedEditorState | null;
  html: string;
  renderedRevision: number;
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
  loadState?: DocumentLoadState;
  loadError?: string | null;
};

function createTab(document: CreateTabInput = {}): EditorTab {
  const content = document.content ?? '';

  return {
    id: createId(),
    path: document.path ?? null,
    title: document.title ?? 'Untitled.md',
    content,
    contentRevision: 0,
    isDirty: document.isDirty ?? false,
    isWelcome: document.isWelcome ?? false,
    loadState: document.loadState ?? 'loaded',
    loadError: document.loadError ?? null,
    lastSavedAt: document.lastSavedAt ?? null,
    fileSize: document.fileSize ?? null,
    cursorPosition: { line: 1, column: 1 },
    scrollPosition: { line: 1, ratio: 0, totalLines: 1, scrollTop: 0, scrollHeight: 0, clientHeight: 0 },
    editorState: null,
    html: '',
    renderedRevision: -1,
    outline: [],
    stats: {
      ...emptyStats,
      characterCount: content.length,
      lineCount: content ? content.split('\n').length : 1
    }
  };
}

function titleFromPath(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).at(-1) ?? path;
}

function createDeferredTab(path: string): EditorTab {
  return createTab({
    path,
    title: titleFromPath(path),
    content: '',
    isDirty: false,
    isWelcome: false,
    loadState: 'unloaded'
  });
}

function hydrateTab(tab: EditorTab, document: DocumentDto): EditorTab {
  const content = document.content;
  return {
    ...tab,
    path: document.path,
    title: document.title,
    content,
    contentRevision: 0,
    isDirty: document.isDirty,
    isWelcome: false,
    loadState: 'loaded',
    loadError: null,
    lastSavedAt: document.lastSavedAt,
    fileSize: document.fileSize,
    cursorPosition: { line: 1, column: 1 },
    scrollPosition: { line: 1, ratio: 0, totalLines: 1, scrollTop: 0, scrollHeight: 0, clientHeight: 0 },
    editorState: null,
    html: '',
    renderedRevision: -1,
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

function closeTabsFromState(
  state: DocumentState,
  tabIds: Iterable<string>,
  preferredActiveId?: string
): DocumentState {
  const closingIds = new Set(tabIds);
  if (!state.tabs.some((tab) => closingIds.has(tab.id))) return state;

  const activeIndex = state.tabs.findIndex((tab) => tab.id === state.activeTabId);
  const nextTabs = state.tabs.filter((tab) => !closingIds.has(tab.id));
  if (!nextTabs.length) {
    const tab = createBlankTab();
    return { tabs: [tab], activeTabId: tab.id };
  }

  if (state.activeTabId && nextTabs.some((tab) => tab.id === state.activeTabId)) {
    return { tabs: nextTabs, activeTabId: state.activeTabId };
  }

  if (preferredActiveId && nextTabs.some((tab) => tab.id === preferredActiveId)) {
    return { tabs: nextTabs, activeTabId: preferredActiveId };
  }

  const rightNeighbor = state.tabs
    .slice(Math.max(0, activeIndex + 1))
    .find((tab) => !closingIds.has(tab.id));
  const leftNeighbor = state.tabs
    .slice(0, Math.max(0, activeIndex))
    .reverse()
    .find((tab) => !closingIds.has(tab.id));
  return {
    tabs: nextTabs,
    activeTabId: rightNeighbor?.id ?? leftNeighbor?.id ?? nextTabs[0].id
  };
}

export function createDocumentStore() {
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
        const existing = state.tabs.find((tab) =>
          isSameFilePath(tab.path, document.path)
        );
        if (existing) {
          return {
            ...state,
            tabs: state.tabs.map((tab) =>
              tab.id === existing.id && tab.loadState !== 'loaded' ? hydrateTab(tab, document) : tab
            ),
            activeTabId: existing.id
          };
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
    restoreSessionTabs(paths: string[], activePath: string | null) {
      store.update((state) => {
        const restoredTabs: EditorTab[] = [];
        for (const path of paths) {
          if (!path.trim() || restoredTabs.some((tab) => isSameFilePath(tab.path, path))) continue;
          restoredTabs.push(createDeferredTab(path));
        }
        if (!restoredTabs.length) return state;

        if (state.tabs.length === 1 && canReplacePlaceholder(state.tabs[0])) {
          const restoredActive = activePath
            ? restoredTabs.find((tab) => isSameFilePath(tab.path, activePath))
            : undefined;
          return {
            tabs: restoredTabs,
            activeTabId: restoredActive?.id ?? restoredTabs[0].id
          };
        }

        const additions = restoredTabs.filter(
          (restored) => !state.tabs.some((tab) => isSameFilePath(tab.path, restored.path))
        );
        return additions.length ? { ...state, tabs: [...state.tabs, ...additions] } : state;
      });
    },
    setActive(id: string) {
      store.update((state) =>
        state.tabs.some((tab) => tab.id === id) ? { ...state, activeTabId: id } : state
      );
    },
    markLoading(tabId: string): boolean {
      let applied = false;
      store.update((state) => ({
        ...state,
        tabs: state.tabs.map((tab) => {
          if (tab.id !== tabId || tab.loadState === 'loaded') return tab;
          applied = true;
          return { ...tab, loadState: 'loading', loadError: null };
        })
      }));
      return applied;
    },
    markLoadFailed(tabId: string, message: string): boolean {
      let applied = false;
      store.update((state) => ({
        ...state,
        tabs: state.tabs.map((tab) => {
          if (tab.id !== tabId || tab.loadState === 'loaded') return tab;
          applied = true;
          return { ...tab, loadState: 'error', loadError: message };
        })
      }));
      return applied;
    },
    hydrateDocument(tabId: string, requestedPath: string, document: DocumentDto): boolean {
      let applied = false;
      store.update((state) => ({
        ...state,
        tabs: state.tabs.map((tab) => {
          if (
            tab.id !== tabId ||
            tab.loadState === 'loaded' ||
            !isSameFilePath(tab.path, requestedPath) ||
            !isSameFilePath(document.path, requestedPath)
          ) {
            return tab;
          }
          applied = true;
          return hydrateTab(tab, document);
        })
      }));
      return applied;
    },
    updateContent(tabId: string, content: string) {
      store.update((state) => ({
        ...state,
        tabs: state.tabs.map((tab) =>
          tab.id === tabId && tab.loadState === 'loaded' && tab.content !== content
            ? {
                ...tab,
                content,
                contentRevision: tab.contentRevision + 1,
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
    updateCursor(tabId: string, cursorPosition: CursorPosition) {
      store.update((state) => ({
        ...state,
        tabs: state.tabs.map((tab) => (tab.id === tabId ? { ...tab, cursorPosition } : tab))
      }));
    },
    updateScroll(tabId: string, scrollPosition: EditorScrollPosition) {
      store.update((state) => ({
        ...state,
        tabs: state.tabs.map((tab) => (tab.id === tabId ? { ...tab, scrollPosition } : tab))
      }));
    },
    updateEditorState(tabId: string, editorState: SerializedEditorState) {
      store.update((state) => ({
        ...state,
        tabs: state.tabs.map((tab) => (tab.id === tabId ? { ...tab, editorState } : tab))
      }));
    },
    updateRendered(tabId: string, contentRevision: number, rendered: RenderedMarkdownDto): boolean {
      let applied = false;
      store.update((state) => ({
        ...state,
        tabs: state.tabs.map((tab) => {
          if (tab.id !== tabId || tab.loadState !== 'loaded' || tab.contentRevision !== contentRevision) return tab;
          applied = true;
          return {
            ...tab,
            html: rendered.html,
            renderedRevision: contentRevision,
            outline: rendered.outline,
            stats: rendered.stats
          };
        })
      }));
      return applied;
    },
    markSaved(tabId: string, contentRevision: number, document: DocumentDto): boolean {
      let applied = false;
      store.update((state) => ({
        ...state,
        tabs: state.tabs.map((tab) => {
          if (tab.id !== tabId || tab.loadState !== 'loaded') return tab;
          applied = true;
          return {
            ...tab,
            path: document.path,
            title: document.title,
            isDirty: tab.contentRevision === contentRevision ? false : tab.isDirty,
            isWelcome: false,
            lastSavedAt: document.lastSavedAt,
            fileSize: document.fileSize
          };
        })
      }));
      return applied;
    },
    getTab(tabId: string): EditorTab | undefined {
      return get(store).tabs.find((tab) => tab.id === tabId);
    },
    isCurrentRevision(tabId: string, contentRevision: number): boolean {
      return get(store).tabs.some(
        (tab) => tab.id === tabId && tab.loadState === 'loaded' && tab.contentRevision === contentRevision
      );
    },
    closeTab(id: string) {
      store.update((state) => closeTabsFromState(state, [id]));
    },
    closeTabs(ids: string[], preferredActiveId?: string) {
      store.update((state) => closeTabsFromState(state, ids, preferredActiveId));
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
