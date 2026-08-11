import { writable } from 'svelte/store';
import {
  clampUnitRatio,
  DEFAULT_SPLIT_RATIO,
  splitDropMode
} from '../../lib/splitPane';
import {
  clampExpandedSidebarWidth,
  DEFAULT_SIDEBAR_WIDTH,
  commitSidebarWidth as sidebarDropResult
} from '../../lib/sidebarResize';

export type LayoutMode = 'edit' | 'split' | 'preview';
export type SidebarTab = 'recent' | 'outline' | 'info';
export type ToastTone = 'info' | 'success' | 'error';

export type Toast = {
  id: string;
  message: string;
  tone: ToastTone;
};

export type UiState = {
  layoutMode: LayoutMode;
  splitRatio: number;
  lastSplitRatio: number;
  sidebarVisible: boolean;
  sidebarWidth: number;
  lastSidebarWidth: number;
  sidebarTab: SidebarTab;
  settingsOpen: boolean;
  commandPaletteOpen: boolean;
  aboutOpen: boolean;
  toasts: Toast[];
};

function createId(): string {
  return `${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

const store = writable<UiState>({
  layoutMode: 'split',
  splitRatio: DEFAULT_SPLIT_RATIO,
  lastSplitRatio: DEFAULT_SPLIT_RATIO,
  sidebarVisible: true,
  sidebarWidth: DEFAULT_SIDEBAR_WIDTH,
  lastSidebarWidth: DEFAULT_SIDEBAR_WIDTH,
  sidebarTab: 'recent',
  settingsOpen: false,
  commandPaletteOpen: false,
  aboutOpen: false,
  toasts: []
});

export const uiStore = {
  subscribe: store.subscribe
};

export const uiActions = {
  setLayoutMode(layoutMode: LayoutMode) {
    store.update((state) => ({
      ...state,
      layoutMode,
      splitRatio: layoutMode === 'split' ? state.lastSplitRatio : state.splitRatio
    }));
  },
  togglePreview() {
    store.update((state) => {
      const layoutMode = state.layoutMode === 'preview' ? 'split' : 'preview';
      return {
        ...state,
        layoutMode,
        splitRatio: layoutMode === 'split' ? state.lastSplitRatio : state.splitRatio
      };
    });
  },
  setSplitRatio(ratio: number) {
    store.update((state) => {
      const splitRatio = clampUnitRatio(ratio);
      return {
        ...state,
        splitRatio,
        lastSplitRatio:
          splitDropMode(splitRatio) === 'split' ? splitRatio : state.lastSplitRatio
      };
    });
  },
  commitSplitRatio(rawRatio: number, visibleRatio: number) {
    store.update((state) => {
      const layoutMode = splitDropMode(rawRatio);
      if (layoutMode === 'split') {
        const splitRatio = clampUnitRatio(visibleRatio);
        return { ...state, layoutMode, splitRatio, lastSplitRatio: splitRatio };
      }
      return { ...state, layoutMode, splitRatio: state.lastSplitRatio };
    });
  },
  toggleSidebar() {
    store.update((state) => ({
      ...state,
      sidebarVisible: !state.sidebarVisible,
      sidebarWidth: !state.sidebarVisible ? state.lastSidebarWidth : state.sidebarWidth
    }));
  },
  setSidebarVisible(sidebarVisible: boolean) {
    store.update((state) => ({
      ...state,
      sidebarVisible,
      sidebarWidth: sidebarVisible ? state.lastSidebarWidth : state.sidebarWidth
    }));
  },
  setSidebarWidth(sidebarWidth: number) {
    store.update((state) => ({ ...state, sidebarWidth }));
  },
  commitSidebarWidth(rawWidth: number, visibleWidth: number, workspaceWidth: number) {
    store.update((state) => {
      const result = sidebarDropResult(rawWidth, workspaceWidth);
      if (result.mode === 'collapse') {
        return { ...state, sidebarVisible: false, sidebarWidth: state.lastSidebarWidth };
      }
      const sidebarWidth = clampExpandedSidebarWidth(visibleWidth, workspaceWidth);
      return {
        ...state,
        sidebarVisible: true,
        sidebarWidth,
        lastSidebarWidth: sidebarWidth
      };
    });
  },
  collapseSidebar() {
    store.update((state) => ({
      ...state,
      sidebarVisible: false,
      sidebarWidth: state.lastSidebarWidth
    }));
  },
  setSidebarTab(sidebarTab: SidebarTab) {
    store.update((state) => ({ ...state, sidebarTab }));
  },
  openSettings() {
    store.update((state) => ({ ...state, settingsOpen: true }));
  },
  closeSettings() {
    store.update((state) => ({ ...state, settingsOpen: false }));
  },
  openCommandPalette() {
    store.update((state) => ({ ...state, commandPaletteOpen: true }));
  },
  closeCommandPalette() {
    store.update((state) => ({ ...state, commandPaletteOpen: false }));
  },
  openAbout() {
    store.update((state) => ({ ...state, aboutOpen: true }));
  },
  closeAbout() {
    store.update((state) => ({ ...state, aboutOpen: false }));
  },
  toast(message: string, tone: ToastTone = 'success') {
    const toast = { id: createId(), message, tone };
    store.update((state) => ({ ...state, toasts: [...state.toasts, toast] }));
    window.setTimeout(() => {
      store.update((state) => ({ ...state, toasts: state.toasts.filter((item) => item.id !== toast.id) }));
    }, 3200);
  },
  dismissToast(id: string) {
    store.update((state) => ({ ...state, toasts: state.toasts.filter((toast) => toast.id !== id) }));
  }
};
