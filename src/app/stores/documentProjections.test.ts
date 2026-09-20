import { get } from 'svelte/store';
import { describe, expect, it, vi } from 'vitest';
import {
  type DocumentDto,
  type SanitizedMarkdownHtml
} from '../../lib/tauriApi';
import { createDocumentProjections, createDocumentStore } from './documentStore';

function document(path: string, content: string): DocumentDto {
  return {
    path,
    fileIdentity: `test-file:${path}`,
    contentVersion: `sha256:${content}`,
    title: path.split(/[\\/]/).at(-1) ?? path,
    content,
    isDirty: false,
    lastSavedAt: null,
    fileSize: content.length
  };
}

describe('document store projections', () => {
  it('keeps active content and editor state out of tab and session consumers', () => {
    const store = createDocumentStore();
    const active = store.openDocument(document('C:\\docs\\a.md', 'A'));
    const projections = createDocumentProjections(store);
    const tabObserver = vi.fn();
    const sessionObserver = vi.fn();
    const tabUnsubscribe = projections.tabBarState.subscribe(tabObserver);
    const sessionUnsubscribe = projections.sessionProjection.subscribe(sessionObserver);

    store.updateCursor(active.id, { line: 1, column: 2 });
    store.updateContent(active.id, 'A changed', 1);
    const tabCallsAfterDirtyTransition = tabObserver.mock.calls.length;
    store.updateContent(active.id, 'A changed again', 1);

    expect(get(projections.activeTab).content).toBe('A changed again');
    expect(tabObserver).toHaveBeenCalledTimes(tabCallsAfterDirtyTransition);
    expect(sessionObserver).toHaveBeenCalledTimes(1);
    tabUnsubscribe();
    sessionUnsubscribe();
  });

  it('does not notify the active editor when a background tab changes', () => {
    const store = createDocumentStore();
    const first = store.openDocument(document('C:\\docs\\a.md', 'A'));
    const second = store.openDocument(document('C:\\docs\\b.md', 'B'));
    store.setActive(first.id);
    const { activeTab } = createDocumentProjections(store);
    const observer = vi.fn();
    const unsubscribe = activeTab.subscribe(observer);

    store.updateCursor(second.id, { line: 2, column: 1 });

    expect(observer).toHaveBeenCalledTimes(1);
    expect(get(activeTab).id).toBe(first.id);
    unsubscribe();
  });

  it('notifies only the active document views that own a changed field', () => {
    const store = createDocumentStore();
    const active = store.openDocument(document('C:\\docs\\a.md', 'A'));
    const projections = createDocumentProjections(store);
    const observers = {
      shell: vi.fn(),
      editor: vi.fn(),
      render: vi.fn(),
      preview: vi.fn(),
      sidebar: vi.fn(),
      status: vi.fn()
    };
    const unsubscribes = [
      projections.activeShell.subscribe(observers.shell),
      projections.activeEditor.subscribe(observers.editor),
      projections.activeRender.subscribe(observers.render),
      projections.activePreview.subscribe(observers.preview),
      projections.activeSidebar.subscribe(observers.sidebar),
      projections.activeStatus.subscribe(observers.status)
    ];

    store.updateCursor(active.id, { line: 2, column: 3 });
    expect(observers.shell).toHaveBeenCalledTimes(1);
    expect(observers.editor).toHaveBeenCalledTimes(1);
    expect(observers.render).toHaveBeenCalledTimes(1);
    expect(observers.preview).toHaveBeenCalledTimes(1);
    expect(observers.sidebar).toHaveBeenCalledTimes(2);
    expect(observers.status).toHaveBeenCalledTimes(2);

    store.updateRendered(active.id, 0, {
      html: '<h1 id="a">A</h1>' as SanitizedMarkdownHtml,
      sourceBlocks: [{ startUtf16: 0, endUtf16: 3, startLine: 1, endLine: 1 }],
      diagrams: [{
        diagramId: 'diagram-0-aaaaaaaaaaaa',
        ordinal: 0,
        sourceUtf8: 'flowchart TD\nA-->B',
        sourceSha256: 'a'.repeat(64),
        sourceStartByte: 0,
        sourceEndByte: 18
      }],
      diagramDiagnostics: [],
      outline: [{ level: 1, title: 'A', line: 1, slug: 'a' }],
      stats: {
        wordCount: 1,
        characterCount: 1,
        lineCount: 1,
        headingCount: 1,
        linkCount: 0,
        imageCount: 0
      }
    });
    expect(observers.shell).toHaveBeenCalledTimes(1);
    expect(observers.editor).toHaveBeenCalledTimes(1);
    expect(observers.render).toHaveBeenCalledTimes(1);
    expect(observers.preview).toHaveBeenCalledTimes(2);
    expect(observers.preview.mock.lastCall?.[0].diagrams).toHaveLength(1);
    expect(observers.sidebar).toHaveBeenCalledTimes(3);
    expect(observers.status).toHaveBeenCalledTimes(3);

    store.updateContent(active.id, 'A changed', 1);
    expect(observers.shell).toHaveBeenCalledTimes(2);
    expect(observers.editor).toHaveBeenCalledTimes(2);
    expect(observers.render).toHaveBeenCalledTimes(2);
    expect(observers.preview).toHaveBeenCalledTimes(3);
    expect(observers.sidebar).toHaveBeenCalledTimes(4);
    expect(observers.status).toHaveBeenCalledTimes(4);

    for (const unsubscribe of unsubscribes) unsubscribe();
  });
});
