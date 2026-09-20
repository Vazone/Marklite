import { mount, tick, unmount } from 'svelte';
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';
import { uiActions } from './stores/uiStore';

const mocks = vi.hoisted(() => ({
  editorModuleLoaded: vi.fn(),
  analyzeMarkdown: vi.fn(),
  renderMarkdown: vi.fn(),
  getSession: vi.fn(),
  openMarkdownFile: vi.fn()
}));

vi.mock('../components/editor/MarkdownEditor.svelte', async () => {
  mocks.editorModuleLoaded();
  return { default: (await import('../test/LazyEditorStub.svelte')).default };
});

vi.mock('../lib/tauriApi', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../lib/tauriApi')>();
  return {
    ...actual,
    api: {
      ...actual.api,
      getSettings: vi.fn(async () => actual.defaultSettings),
      getRecentFiles: vi.fn(async () => []),
      getSession: mocks.getSession,
      getStartupFileArg: vi.fn(async () => null),
      openMarkdownFile: mocks.openMarkdownFile,
      analyzeMarkdown: mocks.analyzeMarkdown,
      renderMarkdown: mocks.renderMarkdown,
      updateSession: vi.fn(async (session) => session),
      recordFrontendStartupEvent: vi.fn(async () => undefined)
    }
  };
});

import App from './App.svelte';

let target: HTMLDivElement;
let component: ReturnType<typeof mount> | undefined;

beforeEach(() => {
  mocks.editorModuleLoaded.mockClear();
  mocks.getSession.mockResolvedValue({
    version: 1,
    paths: ['C:\\docs\\restored.md'],
    activePath: 'C:\\docs\\restored.md'
  });
  mocks.openMarkdownFile.mockResolvedValue({
    document: {
      path: 'C:\\docs\\restored.md',
      fileIdentity: 'restored-file',
      contentVersion: 'sha256:restored',
      title: 'restored.md',
      content: '# Restored',
      isDirty: false,
      lastSavedAt: '2026-08-13T12:00:00Z',
      fileSize: 10
    },
    auxiliaryError: null
  });
  mocks.analyzeMarkdown.mockResolvedValue({
    outline: [{ level: 1, title: 'Restored', line: 1, slug: 'restored' }],
    stats: { wordCount: 1, characterCount: 10, lineCount: 1, headingCount: 1, linkCount: 0, imageCount: 0 }
  });
  mocks.renderMarkdown.mockRejectedValue(new Error('full render must stay idle in edit mode'));
  uiActions.setLayoutMode('edit');
  target = document.createElement('div');
  document.body.append(target);
});

afterEach(async () => {
  if (component) await unmount(component);
  component = undefined;
  target.remove();
  uiActions.setLayoutMode('split');
});

describe('App lazy editor startup', () => {
  test('does not load or mount CodeMirror before session restore and creates the restored editor once', async () => {
    component = mount(App, { target });

    expect(mocks.editorModuleLoaded).not.toHaveBeenCalled();
    expect(target.querySelector('[data-testid="lazy-editor"]')).toBeNull();

    await vi.waitFor(() => expect(target.querySelector('.app-shell')?.getAttribute('data-marklite-ready')).toBe('true'));
    expect(target.textContent).toContain('This restored tab has not read its content yet.');
    expect(mocks.editorModuleLoaded).not.toHaveBeenCalled();

    const loadButton = [...target.querySelectorAll<HTMLButtonElement>('button')].find(
      (button) => button.textContent?.trim() === 'Load document'
    )!;
    loadButton.click();
    await tick();

    await vi.waitFor(() => expect(target.querySelectorAll('[data-testid="lazy-editor"]')).toHaveLength(1));
    expect(mocks.openMarkdownFile).toHaveBeenCalledOnce();
    expect(mocks.editorModuleLoaded).toHaveBeenCalledOnce();
    expect(target.querySelector('[data-testid="lazy-editor"]')?.getAttribute('data-tab-id')).toBeTruthy();
    await vi.waitFor(() => expect(mocks.analyzeMarkdown).toHaveBeenCalledWith('# Restored'));
    expect(mocks.renderMarkdown).not.toHaveBeenCalled();
  });
});
