import type { LayoutMode } from '../app/stores/uiStore';

export type PaneVisibility = {
  editor: boolean;
  preview: boolean;
  separator: boolean;
};

export function paneVisibility(layoutMode: LayoutMode, narrowViewport: boolean): PaneVisibility {
  if (layoutMode === 'edit') return { editor: true, preview: false, separator: false };
  if (layoutMode === 'preview') return { editor: false, preview: true, separator: false };
  return narrowViewport
    ? { editor: true, preview: false, separator: false }
    : { editor: true, preview: true, separator: true };
}
