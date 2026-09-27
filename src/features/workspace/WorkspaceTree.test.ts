import { mount, tick, unmount } from 'svelte';
import { writable } from 'svelte/store';
import { afterEach, expect, test, vi } from 'vitest';
import WorkspaceTree from './WorkspaceTree.svelte';
import type { WorkspaceState, WorkspaceStore } from './store';

afterEach(() => { vi.unstubAllGlobals(); document.body.replaceChildren(); });

test('large tree keeps DOM bounded while scrolling and mouse/keyboard share the open callback', async () => {
  vi.stubGlobal('ResizeObserver', class { observe() {} disconnect() {} });
  const entries = Array.from({ length: 50000 }, (_, index) => ({ name: `${index}.md`, relativePath: `${index}.md`,
    kind: 'markdown' as const, resource: null, size: null, issue: null }));
  const state = writable<WorkspaceState>({ root: null, savedRoot: null, recent: [], restoreOnStart: true, supported: true, loading: false,
    issue: null, persistenceIssue: null, nodes: new Map([['', { expanded: true, busy: false, loaded: true,
      revision: 1, generation: 1, entries, next: null, issue: null, watchIssue: null }]]) });
  const open = vi.fn();
  const store = { subscribe: state.subscribe, open } as unknown as WorkspaceStore;
  const component = mount(WorkspaceTree, { target: document.body, props: { store, label: 'Files', moreLabel: 'More', retryLabel: 'Retry', loadingLabel: 'Loading' } });
  await tick();
  const tree = document.querySelector<HTMLElement>('[role="tree"]')!;
  const press = async (key: string) => { tree.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true })); await tick(); };
  expect(document.querySelectorAll('[role="treeitem"]').length).toBeLessThan(32);
  tree.scrollTop = 750000; tree.dispatchEvent(new Event('scroll')); await tick();
  expect(document.querySelectorAll('[role="treeitem"]').length).toBeLessThan(32);
  expect(document.getElementById(tree.getAttribute('aria-activedescendant')!)).not.toBeNull();
  await press('End'); await press('Enter');
  expect(open).toHaveBeenLastCalledWith('49999.md');
  document.getElementById(tree.getAttribute('aria-activedescendant')!)!.click();
  expect(open).toHaveBeenLastCalledWith('49999.md');
  expect(open).toHaveBeenCalledTimes(2);
  await press('Home'); await press('ArrowDown'); await press('Enter');
  expect(open).toHaveBeenLastCalledWith('1.md');
  expect(document.querySelectorAll('[role="treeitem"]').length).toBeLessThan(32);
  await unmount(component);
});

test('desktop tree rows reveal files and folders; Android resources do not show a file-manager menu', async () => {
  vi.stubGlobal('ResizeObserver', class { observe() {} disconnect() {} });
  const entries = [
    { name: 'notes.md', relativePath: 'notes.md', kind: 'markdown' as const,
      resource: { kind: 'desktopFile' as const, path: 'C:\\notes\\notes.md' }, size: 4, issue: null },
    { name: 'chapter', relativePath: 'chapter', kind: 'directory' as const,
      resource: { kind: 'desktopDirectory' as const, path: 'C:\\notes\\chapter' }, size: null, issue: null },
    { name: 'mobile.md', relativePath: 'mobile.md', kind: 'markdown' as const,
      resource: { kind: 'androidDocument' as const, uri: 'content://provider/mobile.md' }, size: 4, issue: null }
  ];
  const state = writable<WorkspaceState>({ root: null, savedRoot: null, recent: [], restoreOnStart: true,
    supported: true, loading: false, issue: null, persistenceIssue: null,
    nodes: new Map([['', { expanded: true, busy: false, loaded: true, revision: 1, generation: 1,
      entries, next: null, issue: null, watchIssue: null }]]) });
  const reveal = vi.fn();
  const component = mount(WorkspaceTree, { target: document.body, props: {
    store: { subscribe: state.subscribe } as unknown as WorkspaceStore, label: 'Files', moreLabel: 'More',
    retryLabel: 'Retry', loadingLabel: 'Loading', onRevealPath: reveal
  } });
  await tick();
  const context = (index: number) => {
    document.querySelectorAll<HTMLElement>('.tree-row')[index].dispatchEvent(
      new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 40, clientY: 50 }));
  };
  context(0); await tick();
  document.querySelector<HTMLButtonElement>('[role="menuitem"]')!.click();
  expect(reveal).toHaveBeenLastCalledWith('C:\\notes\\notes.md');
  context(1); await tick();
  document.querySelector<HTMLButtonElement>('[role="menuitem"]')!.click();
  expect(reveal).toHaveBeenLastCalledWith('C:\\notes\\chapter');
  context(2); await tick();
  expect(document.querySelector('[role="menuitem"]')).toBeNull();
  await unmount(component);
});
