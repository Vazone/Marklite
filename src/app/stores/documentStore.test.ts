import { get } from 'svelte/store';
import { describe, expect, test } from 'vitest';
import type {
  DocumentDto,
  MarkdownAnalysisDto,
  RenderedMarkdownDto,
  SanitizedMarkdownHtml
} from '../../lib/tauriApi';
import { createDocumentStore } from './documentStore';

function document(
  path: string,
  content = '# Saved',
  fileIdentity = `test-file:${path}`
): DocumentDto {
  return {
    path,
    fileIdentity,
    contentVersion: `sha256:${content}`,
    title: path.split(/[\\/]/).at(-1) ?? 'Untitled.md',
    content,
    isDirty: false,
    lastSavedAt: '2026-08-09T00:00:00Z',
    fileSize: content.length
  };
}

function rendered(html: string): RenderedMarkdownDto {
  return {
    html: html as SanitizedMarkdownHtml,
    outline: [],
    sourceBlocks: [],
    diagrams: [],
    diagramDiagnostics: [],
    stats: {
      wordCount: 1,
      characterCount: html.length,
      lineCount: 1,
      headingCount: 0,
      linkCount: 0,
      imageCount: 0
    }
  };
}

describe('document store invariants', () => {
  test('always keeps an active tab after closing the final tab', () => {
    const store = createDocumentStore();
    const initial = get(store);

    store.closeTab(initial.activeTabId!);

    const state = get(store);
    expect(state.tabs).toHaveLength(1);
    expect(state.tabs[0].id).toBe(state.activeTabId);
    expect(state.tabs[0].path).toBeNull();
  });

  test('activates an already-open path instead of creating a duplicate tab', () => {
    const store = createDocumentStore();
    const opened = store.openDocument(document('C:\\docs\\same.md'));
    const first = get(store);
    const firstTabId = first.activeTabId;
    expect(opened.id).toBe(firstTabId);

    store.newDocument();
    const reopened = store.openDocument(document('C:\\docs\\same.md', 'ignored duplicate response'));

    const state = get(store);
    expect(state.tabs).toHaveLength(2);
    expect(state.activeTabId).toBe(firstTabId);
    expect(reopened.id).toBe(firstTabId);
    expect(state.tabs.find((tab) => tab.id === firstTabId)?.content).toBe('# Saved');
  });

  test('uses an opaque backend identity when activating a Windows path alias', () => {
    const store = createDocumentStore();
    store.openDocument(document('C:\\Docs\\same.md', '# Saved', 'windows-file-42'));
    const firstTabId = get(store).activeTabId;

    store.newDocument();
    store.openDocument(document('c:/docs/./SAME.md', 'ignored alias response', 'windows-file-42'));

    const state = get(store);
    expect(state.tabs).toHaveLength(2);
    expect(state.activeTabId).toBe(firstTabId);
    expect(state.tabs.find((tab) => tab.id === firstTabId)?.content).toBe('# Saved');
  });

  test('does not infer POSIX identity from spelling or case', () => {
    const store = createDocumentStore();
    store.openDocument(document('/home/vazone/docs/Note.md', '# Saved', 'unix-file-42'));
    const firstTabId = get(store).activeTabId;

    store.newDocument();
    store.openDocument(
      document('/home/vazone/docs/./Note.md', 'ignored alias response', 'unix-file-42')
    );
    store.openDocument(
      document('/home/vazone/docs/note.md', 'distinct case-sensitive file', 'unix-file-43')
    );

    const state = get(store);
    expect(state.tabs).toHaveLength(3);
    expect(state.activeTabId).not.toBe(firstTabId);
    expect(state.tabs.find((tab) => tab.id === firstTabId)?.content).toBe('# Saved');
    expect(state.tabs.find((tab) => tab.path?.endsWith('/note.md'))?.content).toBe(
      'distinct case-sensitive file'
    );
  });

  test('restores path-only tabs without materializing document content', () => {
    const store = createDocumentStore();

    store.restoreSessionTabs(
      ['C:\\docs\\first.md', 'D:\\notes\\second.markdown', 'C:\\docs\\first.md'],
      'D:\\notes\\second.markdown'
    );

    const state = get(store);
    expect(state.tabs).toHaveLength(2);
    expect(state.tabs.map((tab) => tab.title)).toEqual(['first.md', 'second.markdown']);
    expect(state.tabs.every((tab) => tab.loadState === 'unloaded')).toBe(true);
    expect(state.tabs.every((tab) => tab.content === '' && tab.html === '' && tab.editorState === null)).toBe(true);
    expect(state.tabs.find((tab) => tab.id === state.activeTabId)?.path).toBe('D:\\notes\\second.markdown');
  });

  test('hydrates a restored tab in place and rejects a response for another path', () => {
    const store = createDocumentStore();
    store.restoreSessionTabs(['C:\\docs\\first.md'], 'C:\\docs\\first.md');
    const tabId = get(store).activeTabId!;

    store.markLoading(tabId);
    expect(store.hydrateDocument(tabId, 'C:\\docs\\other.md', document('C:\\docs\\first.md'))).toBe(false);
    expect(store.hydrateDocument(tabId, 'c:/DOCS/first.md', document('C:\\docs\\first.md', '# Loaded'))).toBe(false);
    expect(store.hydrateDocument(tabId, 'C:\\docs\\first.md', document('C:\\canonical\\first.md', '# Loaded'))).toBe(true);

    const tab = store.getTab(tabId)!;
    expect(tab.id).toBe(tabId);
    expect(tab.loadState).toBe('loaded');
    expect(tab.content).toBe('# Loaded');
    expect(tab.isDirty).toBe(false);
  });

  test('hydrates an existing restored path instead of creating a duplicate tab', () => {
    const store = createDocumentStore();
    store.restoreSessionTabs(['C:\\Docs\\same.md'], null);
    const tabId = get(store).activeTabId!;

    store.openDocument(document('C:\\Docs\\same.md', '# Loaded once'));

    const state = get(store);
    expect(state.tabs).toHaveLength(1);
    expect(state.activeTabId).toBe(tabId);
    expect(state.tabs[0].content).toBe('# Loaded once');
    expect(state.tabs[0].loadState).toBe('loaded');
  });

  test('replaces a loaded dirty buffer only through the explicit reload action', () => {
    const store = createDocumentStore();
    const opened = store.openDocument(document('C:\\docs\\note.md', 'opened'));
    store.updateContent(opened.id, 'my edits');

    expect(store.reloadDocument(
      opened.id,
      'C:\\docs\\note.md',
      document('C:\\docs\\note.md', 'external', 'same-file')
    )).toBe(true);

    expect(store.getTab(opened.id)).toMatchObject({
      content: 'external',
      isDirty: false,
      fileIdentity: 'same-file',
      contentVersion: 'sha256:external'
    });
  });

  test('collapses a restored alias when hydration discovers an existing identity owner', () => {
    const store = createDocumentStore();
    store.restoreSessionTabs(['C:\\docs\\note.md', 'C:\\alias\\note.md'], null);
    const [firstId, aliasId] = get(store).tabs.map((tab) => tab.id);

    store.markLoading(firstId);
    expect(
      store.hydrateDocument(
        firstId,
        'C:\\docs\\note.md',
        document('C:\\docs\\note.md', 'owner', 'windows-file-42')
      )
    ).toBe(true);
    store.setActive(aliasId);
    store.markLoading(aliasId);
    expect(
      store.hydrateDocument(
        aliasId,
        'C:\\alias\\note.md',
        document('C:\\docs\\note.md', 'duplicate', 'windows-file-42')
      )
    ).toBe(true);

    const state = get(store);
    expect(state.tabs).toHaveLength(1);
    expect(state.tabs[0].id).toBe(firstId);
    expect(state.tabs[0].content).toBe('owner');
    expect(state.activeTabId).toBe(firstId);
  });

  test('keeps a failed restored tab retryable without treating it as an empty document', () => {
    const store = createDocumentStore();
    store.restoreSessionTabs(['C:\\docs\\missing.md'], null);
    const tabId = get(store).activeTabId!;

    expect(store.markLoading(tabId)).toBe(true);
    expect(store.markLoadFailed(tabId, '文件不存在')).toBe(true);

    const tab = store.getTab(tabId)!;
    expect(tab.loadState).toBe('error');
    expect(tab.loadError).toBe('文件不存在');
    expect(tab.isDirty).toBe(false);
    expect(tab.content).toBe('');
  });

  test('selects the immediate right neighbor when closing the active tab, then the left neighbor', () => {
    const store = createDocumentStore();
    store.openDocument(document('C:\\docs\\first.md'));
    const firstId = get(store).activeTabId!;
    store.openDocument(document('C:\\docs\\second.md'));
    const secondId = get(store).activeTabId!;
    store.openDocument(document('C:\\docs\\third.md'));
    const thirdId = get(store).activeTabId!;

    store.setActive(secondId);
    store.closeTab(secondId);
    expect(get(store).activeTabId).toBe(thirdId);

    store.closeTab(thirdId);
    expect(get(store).activeTabId).toBe(firstId);
  });

  test('closes multiple tabs atomically and uses the preferred survivor if the active tab closes', () => {
    const store = createDocumentStore();
    for (const name of ['first', 'second', 'third', 'fourth']) {
      store.openDocument(document(`C:\\docs\\${name}.md`));
    }
    const before = get(store);
    const secondId = before.tabs[1].id;
    const rightIds = before.tabs.slice(2).map((tab) => tab.id);

    store.closeTabs(rightIds, secondId);

    const after = get(store);
    expect(after.tabs.map((tab) => tab.title)).toEqual(['first.md', 'second.md']);
    expect(after.activeTabId).toBe(secondId);
  });

  test('closes unloaded tabs without hydrating them and keeps the preferred tab active', () => {
    const store = createDocumentStore();
    store.restoreSessionTabs(
      ['C:\\docs\\first.md', 'C:\\docs\\second.md', 'C:\\docs\\third.md'],
      'C:\\docs\\first.md'
    );
    const before = get(store);
    const keepId = before.tabs[1].id;

    store.closeTabs(
      before.tabs.filter((tab) => tab.id !== keepId).map((tab) => tab.id),
      keepId
    );

    const after = get(store);
    expect(after.tabs).toHaveLength(1);
    expect(after.tabs[0].id).toBe(keepId);
    expect(after.tabs[0].loadState).toBe('unloaded');
    expect(after.activeTabId).toBe(keepId);
  });
});

describe('async response ownership', () => {
  test('counts Unicode scalar values immediately and applies only current semantic analysis', () => {
    const store = createDocumentStore();
    const tabId = get(store).activeTabId!;
    store.updateContent(tabId, '😀é\n', 2);
    const currentRevision = store.getTab(tabId)!.contentRevision;
    expect(store.getTab(tabId)?.stats.characterCount).toBe(4);

    const analysis: MarkdownAnalysisDto = {
      outline: [{ level: 1, title: 'Current', line: 1, slug: 'current' }],
      stats: { wordCount: 1, characterCount: 4, lineCount: 2, headingCount: 1, linkCount: 0, imageCount: 0 }
    };
    expect(store.updateAnalysis(tabId, currentRevision, analysis)).toBe(true);
    store.updateContent(tabId, 'newer', 1);
    expect(store.updateAnalysis(tabId, currentRevision, { ...analysis, outline: [] })).toBe(false);
    expect(store.getTab(tabId)?.outline[0]?.title).toBe('Current');
  });

  test('accepts the editor line count without rescanning content in the store hot path', () => {
    const store = createDocumentStore();
    const tabId = get(store).activeTabId!;

    store.updateContent(tabId, 'first\nsecond\n', 3);

    expect(store.getTab(tabId)?.stats.lineCount).toBe(3);
  });

  test('keeps a save response bound to the tab and revision that started the request', () => {
    const store = createDocumentStore();
    store.openDocument(document('C:\\docs\\first.md', 'request snapshot'));
    const firstTabId = get(store).activeTabId!;
    const requestRevision = store.getTab(firstTabId)!.contentRevision;
    store.updateContent(firstTabId, 'edited while save is pending');
    store.newDocument();
    const secondTabId = get(store).activeTabId!;

    store.markSaved(firstTabId, requestRevision, document('C:\\docs\\first.md', 'request snapshot'));

    const state = get(store);
    const first = state.tabs.find((tab) => tab.id === firstTabId)!;
    const second = state.tabs.find((tab) => tab.id === secondTabId)!;
    expect(first.content).toBe('edited while save is pending');
    expect(first.isDirty).toBe(true);
    expect(second.path).toBeNull();
  });

  test('clears dirty only when the saved revision is still current', () => {
    const store = createDocumentStore();
    store.openDocument(document('C:\\docs\\first.md'));
    const tabId = get(store).activeTabId!;
    store.updateContent(tabId, 'current content');
    const requestRevision = store.getTab(tabId)!.contentRevision;

    store.markSaved(tabId, requestRevision, document('C:\\docs\\first.md', 'current content'));

    const tab = store.getTab(tabId)!;
    expect(tab.content).toBe('current content');
    expect(tab.isDirty).toBe(false);
    expect(tab.contentVersion).toBe('sha256:current content');
  });

  test('applies Save As metadata to the requesting tab without replacing its content', () => {
    const store = createDocumentStore();
    store.newDocument();
    const tabId = get(store).activeTabId!;
    store.updateContent(tabId, 'save-as content');
    const requestRevision = store.getTab(tabId)!.contentRevision;

    store.markSaved(tabId, requestRevision, document('D:\\notes\\renamed.md', 'save-as content'));

    const tab = store.getTab(tabId)!;
    expect(tab.path).toBe('D:\\notes\\renamed.md');
    expect(tab.title).toBe('renamed.md');
    expect(tab.content).toBe('save-as content');
    expect(tab.isDirty).toBe(false);
  });

  test('rejects Save As metadata when another tab owns the same real file', () => {
    const store = createDocumentStore();
    const owner = store.openDocument(
      document('C:\\docs\\owned.md', 'owner', 'windows-file-42')
    );
    store.newDocument();
    const requesterId = get(store).activeTabId!;
    store.updateContent(requesterId, 'must remain unsaved');
    const requestRevision = store.getTab(requesterId)!.contentRevision;

    const applied = store.markSaved(
      requesterId,
      requestRevision,
      document('C:\\alias\\owned.md', 'must remain unsaved', 'windows-file-42')
    );

    expect(applied).toBe(false);
    const requester = store.getTab(requesterId)!;
    expect(requester.path).toBeNull();
    expect(requester.fileIdentity).toBeNull();
    expect(requester.isDirty).toBe(true);
    expect(store.getFileOwner('windows-file-42')?.id).toBe(owner.id);
    expect(store.hasFileIdentityConflict(requesterId, 'windows-file-42')).toBe(true);
    expect(store.hasFileIdentityConflict(owner.id, 'windows-file-42')).toBe(false);
  });

  test('rejects a stale render response that arrives after a newer response', () => {
    const store = createDocumentStore();
    const tabId = get(store).activeTabId!;
    const staleRevision = store.getTab(tabId)!.contentRevision;
    store.updateContent(tabId, 'new content');
    const currentRevision = store.getTab(tabId)!.contentRevision;

    expect(store.updateRendered(tabId, currentRevision, rendered('<p>newest</p>'))).toBe(true);
    expect(store.updateRendered(tabId, staleRevision, rendered('<p>stale</p>'))).toBe(false);

    expect(get(store).tabs[0].html).toBe('<p>newest</p>');
  });

  test('advances a loaded tab revision on reload and rejects both old result kinds', () => {
    const store = createDocumentStore();
    const opened = store.openDocument(document('C:\\docs\\reload.md', '# First'));
    const oldRevision = store.getTab(opened.id)!.contentRevision;
    expect(store.updateRendered(opened.id, oldRevision, rendered('<p>First</p>'))).toBe(true);

    expect(store.reloadDocument(
      opened.id,
      'C:\\docs\\reload.md',
      document('C:\\docs\\reload.md', '# External')
    )).toBe(true);

    const reloaded = store.getTab(opened.id)!;
    expect(reloaded.contentRevision).toBeGreaterThan(oldRevision);
    expect(reloaded.content).toBe('# External');
    expect(reloaded.html).toBe('');
    expect(reloaded.renderedRevision).toBe(-1);
    expect(reloaded.analysisRevision).toBe(-1);
    expect(reloaded.isDirty).toBe(false);
    expect(store.isCurrentRevision(opened.id, oldRevision)).toBe(false);
    expect(store.updateRendered(opened.id, oldRevision, rendered('<p>First late</p>'))).toBe(false);
    expect(store.updateAnalysis(opened.id, oldRevision, rendered('<p>First late</p>'))).toBe(false);
    expect(store.updateRendered(opened.id, reloaded.contentRevision, rendered('<p>External</p>'))).toBe(true);
    expect(store.getTab(opened.id)?.html).toBe('<p>External</p>');

    store.updateContent(opened.id, '# Edited again');
    expect(store.getTab(opened.id)?.contentRevision).toBeGreaterThan(reloaded.contentRevision);
    expect(store.getTab(opened.id)?.isDirty).toBe(true);
  });

  test('keeps reload revisions monotonic across tab switches and isolates a reopened tab', () => {
    const store = createDocumentStore();
    const first = store.openDocument(document('C:\\docs\\reload.md', '# First'));
    const firstRevision = store.getTab(first.id)!.contentRevision;
    store.newDocument();
    store.setActive(first.id);
    expect(store.reloadDocument(first.id, 'C:\\docs\\reload.md', document('C:\\docs\\reload.md', '# Second'))).toBe(true);
    const secondRevision = store.getTab(first.id)!.contentRevision;
    expect(secondRevision).toBeGreaterThan(firstRevision);
    store.closeTab(first.id);

    const reopened = store.openDocument(document('C:\\docs\\reload.md', '# Third'));
    expect(reopened.id).not.toBe(first.id);
    expect(store.updateRendered(first.id, secondRevision, rendered('<p>Second late</p>'))).toBe(false);
    expect(store.getTab(reopened.id)?.content).toBe('# Third');
  });
});
