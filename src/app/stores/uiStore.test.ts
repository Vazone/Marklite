import { get } from 'svelte/store';
import { afterEach, describe, expect, test } from 'vitest';
import { uiActions, uiStore } from './uiStore';

afterEach(() => {
  uiActions.setSplitRatio(0.5);
  uiActions.commitSplitRatio(0.5, 0.5);
  uiActions.setLayoutMode('split');
  uiActions.setSidebarVisible(true);
  uiActions.commitSidebarWidth(280, 280, 1200);
  uiActions.closeModals();
  for (const toast of get(uiStore).toasts) uiActions.dismissToast(toast.id);
});

describe('uiStore modal ownership', () => {
  test('keeps exactly one store-owned modal active', () => {
    uiActions.openSettings();
    expect(get(uiStore)).toMatchObject({
      settingsOpen: true,
      commandPaletteOpen: false,
      aboutOpen: false,
      markdownGuideOpen: false
    });

    uiActions.openCommandPalette();
    expect(get(uiStore)).toMatchObject({
      settingsOpen: false,
      commandPaletteOpen: true,
      aboutOpen: false,
      markdownGuideOpen: false
    });

    uiActions.openAbout();
    expect(get(uiStore)).toMatchObject({
      settingsOpen: false,
      commandPaletteOpen: false,
      aboutOpen: true,
      markdownGuideOpen: false
    });

    uiActions.openMarkdownGuide();
    expect(get(uiStore)).toMatchObject({
      settingsOpen: false,
      commandPaletteOpen: false,
      aboutOpen: false,
      markdownGuideOpen: true
    });
  });
});

describe('uiStore toast ownership', () => {
  test('bounds a burst and releases removed toast timers', () => {
    for (let index = 0; index < 20; index += 1) {
      uiActions.toast(`message-${index}`, 'error');
    }

    const toasts = get(uiStore).toasts;
    expect(toasts).toHaveLength(8);
    expect(toasts[0].message).toBe('message-12');
    expect(toasts.at(-1)?.message).toBe('message-19');
  });
});

describe('uiStore sidebar session state', () => {
  test('collapses only on commit and restores the last expanded width', () => {
    uiActions.setSidebarWidth(150);
    expect(get(uiStore)).toMatchObject({ sidebarVisible: true, sidebarWidth: 150 });

    uiActions.commitSidebarWidth(150, 150, 1200);
    expect(get(uiStore)).toMatchObject({
      sidebarVisible: false,
      sidebarWidth: 280,
      lastSidebarWidth: 280
    });

    uiActions.toggleSidebar();
    expect(get(uiStore)).toMatchObject({ sidebarVisible: true, sidebarWidth: 280 });
  });

  test('snaps 160-200px to 200px and keeps a committed expanded width', () => {
    uiActions.commitSidebarWidth(180, 180, 1200);
    expect(get(uiStore)).toMatchObject({
      sidebarVisible: true,
      sidebarWidth: 200,
      lastSidebarWidth: 200
    });

    uiActions.commitSidebarWidth(420, 420, 800);
    expect(get(uiStore)).toMatchObject({ sidebarWidth: 360, lastSidebarWidth: 360 });
  });
});

describe('uiStore split pane session state', () => {
  test('starts at 50/50 and restores the last split ratio after either single-pane mode', () => {
    expect(get(uiStore)).toMatchObject({
      layoutMode: 'split',
      splitRatio: 0.5,
      lastSplitRatio: 0.5
    });

    uiActions.setSplitRatio(0.63);
    uiActions.setLayoutMode('preview');
    uiActions.setLayoutMode('split');
    expect(get(uiStore).splitRatio).toBe(0.63);
    uiActions.setLayoutMode('edit');
    uiActions.setLayoutMode('split');
    expect(get(uiStore).splitRatio).toBe(0.63);
  });

  test('commits inclusive edge gestures to the correct single-pane mode', () => {
    uiActions.setSplitRatio(0.24);
    uiActions.commitSplitRatio(0.15, 0.16);
    expect(get(uiStore)).toMatchObject({
      layoutMode: 'preview',
      splitRatio: 0.24,
      lastSplitRatio: 0.24
    });

    uiActions.setLayoutMode('split');
    uiActions.commitSplitRatio(0.85, 0.84);
    expect(get(uiStore)).toMatchObject({
      layoutMode: 'edit',
      splitRatio: 0.24,
      lastSplitRatio: 0.24
    });
  });

  test('commits a middle gesture as the new session ratio', () => {
    uiActions.commitSplitRatio(0.61, 0.61);

    expect(get(uiStore)).toMatchObject({
      layoutMode: 'split',
      splitRatio: 0.61,
      lastSplitRatio: 0.61
    });
  });
});
