import { mount, tick, unmount } from 'svelte';
import { writable } from 'svelte/store';
import { afterEach, expect, test, vi } from 'vitest';
import WorkspacePanel from './WorkspacePanel.svelte';
import type { WorkspaceState, WorkspaceStore } from './store';

vi.mock('lucide-svelte', async () => {
  const { default: Icon } = await import('../../test/IconStub.svelte');
  return { CircleAlert: Icon, FileClock: Icon, FileText: Icon, FolderClosed: Icon, FolderOpen: Icon, FolderPlus: Icon,
    RefreshCw: Icon, LocateFixed: Icon, X: Icon };
});

afterEach(() => { document.body.replaceChildren(); vi.unstubAllGlobals(); });

test('folder toolbar uses the injected store and unsupported platforms cannot choose a folder', async () => {
  const state = writable<WorkspaceState>({ root: null, savedRoot: null, recent: [], restoreOnStart: true, supported: false, loading: false,
    issue: null, persistenceIssue: null, nodes: new Map() });
  const choose = vi.fn();
  const store = { subscribe: state.subscribe, choose } as unknown as WorkspaceStore;
  const component = mount(WorkspacePanel, { target: document.body, props: { store } });
  await tick();
  const button = document.querySelector<HTMLButtonElement>('.choose-folder')!;
  expect(button.disabled).toBe(true); button.click(); expect(choose).not.toHaveBeenCalled();
  state.update(value => ({ ...value, supported: true })); await tick();
  button.click(); expect(choose).toHaveBeenCalledOnce();
  await unmount(component);
});

test('repeat locate requests return tree focus after keyboard navigation', async () => {
  vi.stubGlobal('ResizeObserver', class { observe() {} disconnect() {} });
  const state = writable<WorkspaceState>({ root: { rootId: 'root', name: 'Notes', resource: { kind: 'desktopDirectory', path: 'C:\\notes' } },
    savedRoot: null, recent: [], restoreOnStart: true, supported: true, loading: false, issue: null, persistenceIssue: null,
    nodes: new Map([['', { expanded: true, busy: false, loaded: true, revision: 1, generation: 1, next: null, issue: null, watchIssue: null,
      entries: ['one.md', 'two.md'].map(name => ({ name, relativePath: name, kind: 'markdown', resource: null, size: null, issue: null })) }]]) });
  const reveal = vi.fn(async () => true);
  const store = { subscribe: state.subscribe, reveal } as unknown as WorkspaceStore;
  const component = mount(WorkspacePanel, { target: document.body, props: { store, activePath: 'C:\\notes\\one.md' } });
  await tick();
  const locate = document.querySelector<HTMLButtonElement>('button[title="Locate current file"]')!;
  const settle = async () => { for (let i = 0; i < 4; i++) await tick(); };
  locate.click(); await settle();
  const tree = document.querySelector<HTMLElement>('[role="tree"]')!;
  tree.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true })); await tick();
  expect(document.getElementById(tree.getAttribute('aria-activedescendant')!)?.textContent).toContain('two.md');
  locate.click(); await settle();
  expect(document.getElementById(tree.getAttribute('aria-activedescendant')!)?.textContent).toContain('one.md');
  expect(reveal).toHaveBeenCalledTimes(2);
  await unmount(component);
});

test('recent folder choices, removal and independent startup setting are visible and actionable', async () => {
  const first = { kind: 'desktopDirectory' as const, path: 'C:\\notes' };
  const second = { kind: 'desktopDirectory' as const, path: 'C:\\drafts' };
  const state = writable<WorkspaceState>({ root: null, savedRoot: first, recent: [
    { resource: first, name: 'notes' }, { resource: second, name: 'drafts' }], restoreOnStart: false,
    supported: true, loading: false, issue: { code: 'WORKSPACE_NOT_FOUND', message: 'Folder moved' },
    persistenceIssue: null, nodes: new Map() });
  const openRecent = vi.fn(), forgetRecent = vi.fn(), retry = vi.fn(), setRestoreOnStart = vi.fn();
  const store = { subscribe: state.subscribe, openRecent, forgetRecent, retry, setRestoreOnStart } as unknown as WorkspaceStore;
  const component = mount(WorkspacePanel, { target: document.body, props: { store } });
  await tick();
  expect(document.body.textContent).toContain('Folder moved');
  document.querySelector<HTMLButtonElement>('.folder-notice.error button')!.click();
  expect(retry).toHaveBeenCalledOnce();
  document.querySelectorAll<HTMLButtonElement>('.recent-open')[1].click();
  expect(openRecent).toHaveBeenCalledWith(second);
  document.querySelectorAll<HTMLButtonElement>('.recent-forget')[0].click();
  expect(forgetRecent).toHaveBeenCalledWith(first);
  const toggle = document.querySelector<HTMLInputElement>('.restore-setting input')!;
  expect(toggle.checked).toBe(false);
  toggle.checked = true; toggle.dispatchEvent(new Event('change', { bubbles: true }));
  expect(setRestoreOnStart).toHaveBeenCalledWith(true);
  await unmount(component);
});
