import { get, writable, type Readable } from 'svelte/store';
import {
  EMPTY_SANITIZED_MARKDOWN_HTML,
  type DocumentDto,
  type DiagramDiagnostic,
  type DiagramSource,
  type DocumentStats,
  type MarkdownAnalysisDto,
  type MarkdownDiagnostic,
  type OutlineItem,
  type RenderedMarkdownDto,
  type SanitizedMarkdownHtml,
  type SourceBlock,
  type VirtualPreviewIndex
} from '../../lib/tauriApi';
import type { EditorSnapshot } from '../../lib/editorSession';
import { isSameFileIdentity, isSameFilePath } from '../../lib/filePathIdentity';
import { distinctProjection } from '../../lib/distinctProjection';
import { t } from '../../lib/i18n';
import { desktopFile, documentResource, isSameResource, type ResourceRef } from '../../lib/platform/resources';

export type CursorPosition = {
  line: number;
  column: number;
};

export type EditorScrollPosition = {
  line: number;
  offsetUtf16?: number;
  ratio: number;
  totalLines: number;
  scrollTop: number;
  scrollHeight: number;
  clientHeight: number;
};

export type DocumentLoadState = 'loaded' | 'unloaded' | 'loading' | 'error';

export type EditorTab = {
  id: string;
  resource: ResourceRef | null;
  path: string | null;
  fileIdentity: string | null;
  contentVersion: string | null;
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
  editorState: EditorSnapshot | null;
  html: SanitizedMarkdownHtml;
  sourceBlocks: SourceBlock[];
  virtualPreview: VirtualPreviewIndex | null;
  diagrams: DiagramSource[];
  diagramDiagnostics: DiagramDiagnostic[];
  markdownDiagnostics?: MarkdownDiagnostic[];
  renderedRevision: number;
  analysisRevision: number;
  outline: OutlineItem[];
  stats: DocumentStats;
};

export type DocumentState = {
  tabs: EditorTab[];
  activeTabId: string | null;
};

export type DocumentTabDescriptor = Pick<
  EditorTab,
  'id' | 'path' | 'title' | 'isDirty' | 'loadState'
>;

export type DocumentSessionProjection = {
  paths: string[];
  activePath: string | null;
};

export type ResourceSessionProjection = {
  resources: ResourceRef[];
  activeResource: ResourceRef | null;
};

export type ActiveDocumentShell = Pick<
  EditorTab,
  'id' | 'path' | 'title' | 'isDirty' | 'loadState' | 'loadError'
>;

export type ActiveEditorDocument = Pick<
  EditorTab,
  'id' | 'content' | 'editorState' | 'scrollPosition' | 'loadState'
>;

export type ActiveRenderDocument = Pick<
  EditorTab,
  'id' | 'content' | 'contentRevision' | 'loadState'
>;

export type ActivePreviewDocument = Pick<
  EditorTab,
  'id' | 'path' | 'resource' | 'title' | 'html' | 'sourceBlocks' | 'virtualPreview' | 'diagrams' | 'diagramDiagnostics' | 'markdownDiagnostics' | 'analysisRevision' | 'renderedRevision' | 'contentRevision' | 'outline' | 'loadState'
>;

export type SidebarDocumentView = Pick<
  EditorTab,
  | 'id'
  | 'path'
  | 'title'
  | 'fileSize'
  | 'lastSavedAt'
  | 'cursorPosition'
  | 'scrollPosition'
  | 'outline'
  | 'stats'
>;

export type StatusDocumentView = Pick<
  EditorTab,
  'title' | 'path' | 'isDirty' | 'loadState' | 'stats' | 'cursorPosition'
>;

const emptyStats: DocumentStats = {
  wordCount: 0,
  characterCount: 0,
  lineCount: 1,
  headingCount: 0,
  linkCount: 0,
  imageCount: 0
};

function countUnicodeScalars(value: string): number {
  let count = 0;
  for (const _character of value) count += 1;
  return count;
}

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
    resource: documentResource({ path: document.path ?? null, resource: document.resource }),
    fileIdentity: document.fileIdentity ?? null,
    contentVersion: document.contentVersion ?? null,
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
    html: EMPTY_SANITIZED_MARKDOWN_HTML,
    sourceBlocks: [],
    virtualPreview: null,
    diagrams: [],
    diagramDiagnostics: [],
    markdownDiagnostics: [],
    renderedRevision: -1,
    analysisRevision: -1,
    outline: [],
    stats: {
      ...emptyStats,
      characterCount: countUnicodeScalars(content),
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

function createDeferredResourceTab(resource: ResourceRef): EditorTab {
  if (resource.kind === 'desktopFile') return createDeferredTab(resource.path);
  return createTab({ path: null, resource, title: 'Document.md', content: '',
    isDirty: false, isWelcome: false, loadState: 'unloaded' });
}

function hydrateTab(tab: EditorTab, document: DocumentDto): EditorTab {
  const content = document.content;
  const unchanged = tab.loadState !== 'loaded' && tab.contentVersion === document.contentVersion;
  return {
    ...tab,
    path: document.path,
    resource: documentResource({ path: document.path ?? null, resource: document.resource }),
    fileIdentity: document.fileIdentity,
    contentVersion: document.contentVersion,
    title: document.title,
    content,
    contentRevision: tab.loadState === 'loaded' ? tab.contentRevision + 1 : tab.contentRevision,
    isDirty: document.isDirty,
    isWelcome: false,
    loadState: 'loaded',
    loadError: null,
    lastSavedAt: document.lastSavedAt,
    fileSize: document.fileSize,
    cursorPosition: unchanged ? tab.cursorPosition : { line: 1, column: 1 },
    scrollPosition: unchanged ? tab.scrollPosition : { line: 1, ratio: 0, totalLines: 1, scrollTop: 0, scrollHeight: 0, clientHeight: 0 },
    editorState: unchanged ? tab.editorState : null,
    html: EMPTY_SANITIZED_MARKDOWN_HTML,
    sourceBlocks: [],
    virtualPreview: null,
    diagrams: [],
    diagramDiagnostics: [],
    markdownDiagnostics: [],
    renderedRevision: -1,
    analysisRevision: -1,
    outline: [],
    stats: {
      ...emptyStats,
      characterCount: countUnicodeScalars(content),
      lineCount: content ? content.split('\n').length : 1
    }
  };
}

function createWelcomeTab(): EditorTab {
  return createTab({
    title: 'Untitled.md',
    content: t('welcome.content'),
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
    localizeWelcome() {
      const content = t('welcome.content');
      store.update((state) => ({
        ...state,
        tabs: state.tabs.map((tab) => tab.isWelcome && !tab.isDirty && tab.content !== content
          ? {
              ...tab,
              content,
              contentRevision: tab.contentRevision + 1,
              renderedRevision: -1,
              analysisRevision: -1,
              stats: {
                ...tab.stats,
                characterCount: countUnicodeScalars(content),
                lineCount: content.split('\n').length
              }
            }
          : tab)
      }));
    },
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
      let openedTab: EditorTab | undefined;
      store.update((state) => {
        const existing = state.tabs.find(
          (tab) =>
            isSameFileIdentity(tab.fileIdentity, document.fileIdentity) ||
            (tab.fileIdentity === null && isSameFilePath(tab.path, document.path))
        );
        if (existing) {
          openedTab = existing.loadState !== 'loaded' ? hydrateTab(existing, document) : existing;
          return {
            ...state,
            tabs: state.tabs.map((tab) =>
              tab.id === existing.id ? openedTab! : tab
            ),
            activeTabId: existing.id
          };
        }

        const tab = createTab(document);
        openedTab = tab;
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
      if (!openedTab) {
        throw new Error('Document tab creation failed');
      }
      return openedTab;
    },
    restoreSessionTabs(paths: string[], activePath: string | null) {
      store.update((state) => {
        const restoredTabs: EditorTab[] = [];
        for (const path of paths) {
          if (!path || restoredTabs.some((tab) => isSameFilePath(tab.path, path))) continue;
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
    restoreResourceSessionTabs(resources: ResourceRef[], activeResource: ResourceRef | null) {
      store.update((state) => {
        const restored: EditorTab[] = [];
        for (const resource of resources) {
          if ((resource.kind !== 'desktopFile' && resource.kind !== 'androidDocument') ||
            restored.some(tab => isSameResource(tab.resource, resource))) continue;
          restored.push(createDeferredResourceTab(resource));
          if (restored.length === 50) break;
        }
        if (!restored.length) return state;
        if (state.tabs.length === 1 && canReplacePlaceholder(state.tabs[0])) {
          const active = restored.find(tab => isSameResource(tab.resource, activeResource));
          return { tabs: restored, activeTabId: active?.id ?? restored[0].id };
        }
        const additions = restored.filter(tab => !state.tabs.some(existing =>
          isSameResource(existing.resource, tab.resource)));
        return additions.length ? { ...state, tabs: [...state.tabs, ...additions] } : state;
      });
    },
    setDeferredResourceTitle(tabId: string, resource: ResourceRef, title: string): boolean {
      let applied = false;
      store.update((state) => {
        const tabs = state.tabs.map((tab) => {
          if (tab.id !== tabId || tab.loadState === 'loaded' ||
            !isSameResource(tab.resource, resource)) return tab;
          applied = true;
          return { ...tab, title };
        });
        return applied ? { ...state, tabs } : state;
      });
      return applied;
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
    hydrateDocument(tabId: string, requestedTarget: string | ResourceRef, document: DocumentDto): boolean {
      const requestedResource = typeof requestedTarget === 'string' ? desktopFile(requestedTarget) : requestedTarget;
      let applied = false;
      store.update((state) => {
        const target = state.tabs.find((tab) => tab.id === tabId);
        if (
          !target ||
          target.loadState === 'loaded' ||
          !isSameResource(target.resource, requestedResource)
        ) {
          return state;
        }

        const existingOwner = state.tabs.find(
          (tab) =>
            tab.id !== tabId && isSameFileIdentity(tab.fileIdentity, document.fileIdentity)
        );
        applied = true;
        if (existingOwner) {
          return {
            tabs: state.tabs.filter((tab) => tab.id !== tabId),
            activeTabId: state.activeTabId === tabId ? existingOwner.id : state.activeTabId
          };
        }

        return {
          ...state,
          tabs: state.tabs.map((tab) =>
            tab.id === tabId ? hydrateTab(tab, document) : tab
          )
        };
      });
      return applied;
    },
    reloadDocument(tabId: string, requestedTarget: string | ResourceRef, document: DocumentDto): boolean {
      const requestedResource = typeof requestedTarget === 'string' ? desktopFile(requestedTarget) : requestedTarget;
      let applied = false;
      store.update((state) => {
        const target = state.tabs.find((tab) => tab.id === tabId);
        if (!target || !isSameResource(target.resource, requestedResource)) return state;
        const existingOwner = state.tabs.find(
          (tab) => tab.id !== tabId && isSameFileIdentity(tab.fileIdentity, document.fileIdentity)
        );
        applied = true;
        if (existingOwner) {
          return {
            tabs: state.tabs.filter((tab) => tab.id !== tabId),
            activeTabId: state.activeTabId === tabId ? existingOwner.id : state.activeTabId
          };
        }
        return {
          ...state,
          tabs: state.tabs.map((tab) => (tab.id === tabId ? hydrateTab(tab, document) : tab))
        };
      });
      return applied;
    },
    updateContent(tabId: string, content: string, lineCount?: number) {
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
                  characterCount: countUnicodeScalars(content),
                  lineCount: lineCount ?? (content ? content.split('\n').length : 1)
                }
              }
            : tab
        )
      }));
    },
    markDirty(tabId: string) {
      store.update((state) => {
        if (!state.tabs.some((tab) => tab.id === tabId && tab.loadState === 'loaded' && !tab.isDirty)) {
          return state;
        }
        return {
          ...state,
          tabs: state.tabs.map((tab) =>
            tab.id === tabId ? { ...tab, isDirty: true, isWelcome: false } : tab
          )
        };
      });
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
    updateEditorState(tabId: string, editorState: EditorSnapshot) {
      store.update((state) => ({
        ...state,
        tabs: state.tabs.map((tab) => (tab.id === tabId && tab.loadState === 'loaded' ? { ...tab, editorState } : tab))
      }));
    },
    evictClean(tabId: string): { previewSession: string | null } | null {
      let released: { previewSession: string | null } | null = null;
      store.update((state) => ({
        ...state,
        tabs: state.tabs.map((tab) => {
          if (tab.id !== tabId || tab.id === state.activeTabId || tab.isDirty || !tab.resource || tab.loadState !== 'loaded') return tab;
          released = { previewSession: tab.virtualPreview?.sessionId ?? null };
          return {
            ...tab,
            content: '',
            contentRevision: tab.contentRevision + 1,
            loadState: 'unloaded',
            editorState: tab.editorState ? { selection: tab.editorState.selection } : null,
            html: EMPTY_SANITIZED_MARKDOWN_HTML,
            sourceBlocks: [],
            virtualPreview: null,
            diagrams: [],
            diagramDiagnostics: [],
            markdownDiagnostics: [],
            outline: [],
            renderedRevision: -1,
            analysisRevision: -1
          };
        })
      }));
      return released;
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
            sourceBlocks: rendered.sourceBlocks,
            virtualPreview: rendered.virtualPreview ?? null,
            diagrams: rendered.diagrams,
            diagramDiagnostics: rendered.diagramDiagnostics,
            markdownDiagnostics: rendered.markdownDiagnostics ?? [],
            renderedRevision: contentRevision,
            analysisRevision: contentRevision,
            outline: rendered.outline,
            stats: rendered.stats
          };
        })
      }));
      return applied;
    },
    updateAnalysis(tabId: string, contentRevision: number, analysis: MarkdownAnalysisDto): boolean {
      let applied = false;
      store.update((state) => ({
        ...state,
        tabs: state.tabs.map((tab) => {
          if (tab.id !== tabId || tab.loadState !== 'loaded' || tab.contentRevision !== contentRevision) return tab;
          applied = true;
          return {
            ...tab,
            analysisRevision: contentRevision,
            markdownDiagnostics: analysis.markdownDiagnostics ?? [],
            outline: analysis.outline,
            stats: analysis.stats
          };
        })
      }));
      return applied;
    },
    markSaved(tabId: string, contentRevision: number, document: DocumentDto): boolean {
      let applied = false;
      store.update((state) => {
        const conflicts = state.tabs.some(
          (tab) =>
            tab.id !== tabId && isSameFileIdentity(tab.fileIdentity, document.fileIdentity)
        );
        if (conflicts) return state;

        return {
          ...state,
          tabs: state.tabs.map((tab) => {
            if (tab.id !== tabId || tab.loadState !== 'loaded') return tab;
            applied = true;
            return {
              ...tab,
              path: document.path,
              resource: documentResource({ path: document.path ?? null, resource: document.resource }),
              fileIdentity: document.fileIdentity,
              contentVersion: document.contentVersion,
              title: document.title,
              isDirty: tab.contentRevision === contentRevision ? false : tab.isDirty,
              isWelcome: false,
              lastSavedAt: document.lastSavedAt,
              fileSize: document.fileSize
            };
          })
        };
      });
      return applied;
    },
    getFileOwner(fileIdentity: string, excludingTabId?: string): EditorTab | undefined {
      return get(store).tabs.find(
        (tab) =>
          tab.id !== excludingTabId && isSameFileIdentity(tab.fileIdentity, fileIdentity)
      );
    },
    hasFileIdentityConflict(tabId: string, fileIdentity: string | null): boolean {
      return get(store).tabs.some(
        (tab) => tab.id !== tabId && isSameFileIdentity(tab.fileIdentity, fileIdentity)
      );
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

export function createDocumentProjections(store: Readable<DocumentState>) {
  const findActive = (state: DocumentState) =>
    state.tabs.find((tab) => tab.id === state.activeTabId) ?? state.tabs[0];
  const activeTab = distinctProjection(
    store,
    findActive,
    Object.is
  );
  const activeShell = projectActive(store, findActive, ({
    id,
    path,
    title,
    isDirty,
    loadState,
    loadError
  }): ActiveDocumentShell => ({ id, path, title, isDirty, loadState, loadError }));
  const activeEditor = projectActive(store, findActive, ({
    id,
    content,
    editorState,
    scrollPosition,
    loadState
  }): ActiveEditorDocument => ({ id, content, editorState, scrollPosition, loadState }));
  const activeRender = projectActive(store, findActive, ({
    id,
    content,
    contentRevision,
    loadState
  }): ActiveRenderDocument => ({ id, content, contentRevision, loadState }));
  const activePreview = projectActive(store, findActive, ({
    id,
    path,
    resource,
    title,
    html,
    sourceBlocks,
    virtualPreview,
    diagrams,
    diagramDiagnostics,
    markdownDiagnostics,
    analysisRevision,
    renderedRevision,
    contentRevision,
    outline,
    loadState
  }): ActivePreviewDocument => ({ id, path, resource, title, html, sourceBlocks, virtualPreview, diagrams, diagramDiagnostics, markdownDiagnostics, analysisRevision, renderedRevision, contentRevision, outline, loadState }));
  const activeSidebar = projectActive(store, findActive, ({
    id,
    path,
    title,
    fileSize,
    lastSavedAt,
    cursorPosition,
    scrollPosition,
    outline,
    stats
  }): SidebarDocumentView => ({
    id,
    path,
    title,
    fileSize,
    lastSavedAt,
    cursorPosition,
    scrollPosition,
    outline,
    stats
  }));
  const activeStatus = projectActive(store, findActive, ({
    title,
    path,
    isDirty,
    loadState,
    stats,
    cursorPosition
  }): StatusDocumentView => ({ title, path, isDirty, loadState, stats, cursorPosition }));
  const tabBarState = distinctProjection(
    store,
    (state) => ({
      activeTabId: state.activeTabId,
      tabs: state.tabs.map(({ id, path, title, isDirty, loadState }) => ({
        id,
        path,
        title,
        isDirty,
        loadState
      }))
    }),
    (left, right) =>
      left.activeTabId === right.activeTabId &&
      left.tabs.length === right.tabs.length &&
      left.tabs.every((tab, index) => {
        const other = right.tabs[index];
        return (
          tab.id === other.id &&
          tab.path === other.path &&
          tab.title === other.title &&
          tab.isDirty === other.isDirty &&
          tab.loadState === other.loadState
        );
      })
  );
  const sessionProjection = distinctProjection(
    store,
    (state): DocumentSessionProjection => {
      const paths = state.tabs.flatMap((tab) => (tab.path ? [tab.path] : []));
      const activePath =
        state.tabs.find((tab) => tab.id === state.activeTabId)?.path ?? paths[0] ?? null;
      return { paths, activePath };
    },
    (left, right) =>
      left.activePath === right.activePath &&
      left.paths.length === right.paths.length &&
      left.paths.every((path, index) => path === right.paths[index])
  );
  const resourceSessionProjection = distinctProjection(
    store,
    (state): ResourceSessionProjection => {
      const resources = state.tabs.flatMap(tab => tab.resource &&
        (tab.resource.kind === 'desktopFile' || tab.resource.kind === 'androidDocument') ? [tab.resource] : []);
      const activeResource = state.tabs.find(tab => tab.id === state.activeTabId)?.resource ?? null;
      return { resources, activeResource: resources.some(resource => isSameResource(resource, activeResource))
        ? activeResource : null };
    },
    (left, right) =>
      (isSameResource(left.activeResource, right.activeResource) ||
        (left.activeResource === null && right.activeResource === null)) &&
      left.resources.length === right.resources.length &&
      left.resources.every((resource, index) => isSameResource(resource, right.resources[index]))
  );
  return {
    activeTab,
    activeShell,
    activeEditor,
    activeRender,
    activePreview,
    activeSidebar,
    activeStatus,
    tabBarState,
    sessionProjection,
    resourceSessionProjection
  };
}

function projectActive<Value extends Record<string, unknown>>(
  store: Readable<DocumentState>,
  findActive: (state: DocumentState) => EditorTab,
  project: (tab: EditorTab) => Value
): Readable<Value> {
  return distinctProjection(
    store,
    (state) => project(findActive(state)),
    (left, right) => {
      const keys = Object.keys(left) as (keyof Value)[];
      return keys.every((key) => Object.is(left[key], right[key]));
    }
  );
}

export const {
  activeTab,
  activeShell,
  activeEditor,
  activeRender,
  activePreview,
  activeSidebar,
  activeStatus,
  tabBarState,
  sessionProjection,
  resourceSessionProjection
} = createDocumentProjections(documentStore);

export function getActiveTab(): EditorTab {
  return get(activeTab);
}
