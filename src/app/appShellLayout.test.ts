/// <reference types="node" />

import { readFileSync } from 'node:fs';
import { mount, tick, unmount } from 'svelte';
import { get } from 'svelte/store';
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';
import App from './App.svelte';
import { uiActions, uiStore } from './stores/uiStore';
import { settingsStore } from './stores/settingsStore';
import { defaultSettings } from '../lib/tauriApi';

const globalsCss = readFileSync('src/styles/globals.css', 'utf8');
let globalStyle: HTMLStyleElement;
let target: HTMLDivElement;
let appComponent: ReturnType<typeof mount> | undefined;

beforeEach(() => {
  globalStyle = document.createElement('style');
  globalStyle.textContent = globalsCss;
  document.head.append(globalStyle);
  target = document.createElement('div');
  document.body.append(target);
});

afterEach(async () => {
  if (appComponent) await unmount(appComponent);
  appComponent = undefined;
  target.remove();
  globalStyle.remove();
  settingsStore.setLocal({ ...defaultSettings });
});

function renderShell(includeToolbar: boolean): HTMLElement {
  target.innerHTML = `
    <div class="app-shell">
      <div class="titlebar"></div>
      <div class="tabbar"></div>
      ${includeToolbar ? '<div class="markdown-toolbar"></div>' : ''}
      <div class="workspace"></div>
      <div class="statusbar"></div>
    </div>
  `;
  return target.querySelector<HTMLElement>('.app-shell')!;
}

describe('application shell grid', () => {
  test('connects the real App shell, workspace, toolbar and statusbar to their layout classes', async () => {
    appComponent = mount(App, { target });
    await tick();

    const shell = target.querySelector<HTMLElement>('.app-shell')!;
    await vi.waitFor(() => expect(shell.dataset.markliteReady).toBe('true'));
    const toolbar = shell.querySelector<HTMLElement>('.markdown-toolbar')!;
    const workspace = shell.querySelector<HTMLElement>('.workspace')!;
    const statusbar = shell.querySelector<HTMLElement>('.statusbar')!;
    expect(shell).toBeTruthy();
    expect(toolbar).toBeTruthy();
    expect(workspace).toBeTruthy();
    expect(statusbar).toBeTruthy();
    expect(getComputedStyle(toolbar).gridArea).toBe('toolbar');
    expect(getComputedStyle(workspace).gridArea).toBe('workspace');
    expect(getComputedStyle(statusbar).gridArea).toBe('statusbar');
  });

  test('keeps application chrome at a fixed size when editor font size changes', async () => {
    appComponent = mount(App, { target });
    await tick();
    const shell = target.querySelector<HTMLElement>('.app-shell')!;
    await vi.waitFor(() => expect(shell.dataset.markliteReady).toBe('true'));
    const brand = shell.querySelector<HTMLElement>('.brand strong')!;
    const before = getComputedStyle(brand).fontSize;

    settingsStore.setLocal({ ...get(settingsStore), editorFontSize: 24 });
    await tick();

    expect(getComputedStyle(brand).fontSize).toBe(before);
    expect(getComputedStyle(shell.querySelector('.statusbar')!).height).toBe('26px');
  });

  test.each([true, false])(
    'keeps workspace and statusbar in their grid tracks when toolbar mounted=%s',
    (includeToolbar) => {
      const shell = renderShell(includeToolbar);
      const shellStyle = getComputedStyle(shell);

      expect(shellStyle.display).toBe('grid');
      expect(shellStyle.gridTemplateAreas.match(/titlebar|tabbar|toolbar|workspace|statusbar/g)).toEqual([
        'titlebar',
        'tabbar',
        'toolbar',
        'workspace',
        'statusbar'
      ]);
      expect(shellStyle.gridTemplateRows).toBe('56px 44px auto minmax(0, 1fr) auto');
      expect(getComputedStyle(shell.querySelector('.titlebar')!).gridArea).toBe('titlebar');
      expect(getComputedStyle(shell.querySelector('.tabbar')!).gridArea).toBe('tabbar');
      expect(getComputedStyle(shell.querySelector('.workspace')!).gridArea).toBe('workspace');
      expect(getComputedStyle(shell.querySelector('.statusbar')!).gridArea).toBe('statusbar');
    }
  );

  test('applies the fixed tab strip and bounded workspace styles through the DOM', () => {
    const shell = renderShell(true);
    const tabbar = getComputedStyle(shell.querySelector('.tabbar')!);
    const workspace = getComputedStyle(shell.querySelector('.workspace')!);
    const statusbar = getComputedStyle(shell.querySelector('.statusbar')!);

    expect(tabbar.height).toBe('44px');
    expect(tabbar.minHeight).toBe('44px');
    expect(tabbar.overflowY).toBe('hidden');
    expect(tabbar.scrollbarWidth).toBe('thin');
    expect(workspace.minHeight).toBe('0px');
    expect(statusbar.height).toBe('26px');
  });

  test('allows a later session toggle after applying a sidebar visibility preference', () => {
    const initial = get(uiStore);
    try {
      uiActions.setSidebarVisible(false);
      expect(get(uiStore).sidebarVisible).toBe(false);

      uiActions.toggleSidebar();
      expect(get(uiStore).sidebarVisible).toBe(true);
    } finally {
      uiActions.setSidebarWidth(initial.sidebarWidth);
      uiActions.setSidebarVisible(initial.sidebarVisible);
    }
  });
});
