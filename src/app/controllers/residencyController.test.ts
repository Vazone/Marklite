import { EditorSelection } from '@codemirror/state';
import { get } from 'svelte/store';
import { afterEach, expect, test, vi } from 'vitest';
import { createDocumentStore } from '../stores/documentStore';
import { createResidencyController } from './residencyController';
import type { DocumentDto } from '../../lib/tauriApi';

afterEach(() => vi.useRealTimers());

function document(name: string, content = 'some content'): DocumentDto {
  return { path: `C:\\${name}.md`, title: name, content, fileIdentity: name, contentVersion: content,
    isDirty: false, lastSavedAt: null, fileSize: content.length };
}

function setup(budget = { bytes: 1024, documents: 2 }) {
  vi.useFakeTimers();
  const store = createDocumentStore();
  const releasePreview = vi.fn().mockResolvedValue(undefined);
  const controller = createResidencyController({ subscribe: store.subscribe, evict: store.evictClean,
    releasePreview, onError: vi.fn(), budget });
  controller.start();
  return { store, releasePreview, controller };
}

test('evicts least recently active clean content and restores selection on an unchanged file', async () => {
  const { store, controller } = setup();
  const a = store.openDocument(document('a'));
  const b = store.openDocument(document('b'));
  store.setActive(a.id);
  const selection = EditorSelection.single(3, 7);
  store.updateEditorState(b.id, { selection, history: { unused: true } });
  const c = store.openDocument(document('c'));
  await vi.runAllTimersAsync();
  expect(store.getTab(a.id)?.loadState).toBe('loaded');
  expect(store.getTab(b.id)?.loadState).toBe('unloaded');
  expect(store.getTab(b.id)?.content).toBe('');
  expect(store.getTab(b.id)?.editorState).toEqual({ selection });
  expect(store.getTab(c.id)?.loadState).toBe('loaded');
  store.setActive(b.id);
  expect(store.hydrateDocument(b.id, b.path!, document('b'))).toBe(true);
  expect(store.getTab(b.id)?.editorState?.selection).toBe(selection);
  expect(store.isCurrentRevision(b.id, b.contentRevision)).toBe(false);
  controller.dispose();
});

test('protects active, dirty and untitled documents even when the budget is exceeded', async () => {
  const { store, controller } = setup({ bytes: 1, documents: 1 });
  const a = store.openDocument(document('a'));
  store.updateContent(a.id, 'unsaved changes');
  store.newDocument();
  const untitled = get(store).activeTabId!;
  const active = store.openDocument(document('active'));
  await vi.runAllTimersAsync();
  expect(store.getTab(a.id)?.content).toBe('unsaved changes');
  expect(store.getTab(untitled)?.loadState).toBe('loaded');
  expect(store.getTab(active.id)?.content).toBe('some content');
  controller.dispose();
});

test('rejects late render/snapshot writes and changed-file selection after eviction', async () => {
  const { store, controller, releasePreview } = setup({ bytes: 1, documents: 1 });
  const a = store.openDocument(document('a'));
  store.updateEditorState(a.id, { selection: EditorSelection.single(7) });
  const rendered = { html: a.html, sourceBlocks: [], diagrams: [], diagramDiagnostics: [], outline: [], stats: a.stats,
    virtualPreview: { sessionId: 'old-preview', segments: [] } };
  store.updateRendered(a.id, a.contentRevision, rendered);
  store.openDocument(document('b'));
  await vi.runAllTimersAsync();
  expect(releasePreview).toHaveBeenCalledExactlyOnceWith('old-preview');
  expect(store.updateRendered(a.id, a.contentRevision, rendered)).toBe(false);
  store.updateEditorState(a.id, { selection: EditorSelection.single(0), history: { stale: true } });
  expect(store.getTab(a.id)?.editorState?.selection.main.head).toBe(7);
  store.setActive(a.id);
  store.hydrateDocument(a.id, a.path!, document('a', 'new'));
  expect(store.getTab(a.id)?.editorState).toBeNull();
  controller.dispose();
});

test('disposal cancels queued eviction and removes the store observer', async () => {
  const { store, controller } = setup({ bytes: 1, documents: 1 });
  const a = store.openDocument(document('a'));
  store.openDocument(document('b'));
  controller.dispose();
  await vi.runAllTimersAsync();
  expect(store.getTab(a.id)?.loadState).toBe('loaded');
  store.openDocument(document('c'));
  await vi.runAllTimersAsync();
  expect(store.getTab(a.id)?.loadState).toBe('loaded');
});
