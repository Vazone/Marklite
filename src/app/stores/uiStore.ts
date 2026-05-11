import { writable } from 'svelte/store';

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
  sidebarVisible: boolean;
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
  sidebarVisible: true,
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
    store.update((state) => ({ ...state, layoutMode }));
  },
  togglePreview() {
    store.update((state) => ({
      ...state,
      layoutMode: state.layoutMode === 'preview' ? 'split' : 'preview'
    }));
  },
  toggleSidebar() {
    store.update((state) => ({ ...state, sidebarVisible: !state.sidebarVisible }));
  },
  setSidebarVisible(sidebarVisible: boolean) {
    store.update((state) => ({ ...state, sidebarVisible }));
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
