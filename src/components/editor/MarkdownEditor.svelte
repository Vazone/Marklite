<script context="module" lang="ts">
  let editorStartupMountReported = false;
</script>

<script lang="ts">
  import { onDestroy, onMount, tick } from 'svelte';
  import {
    Compartment,
    EditorSelection,
    EditorState,
    RangeSetBuilder,
    StateEffect,
    StateField,
    type Text
  } from '@codemirror/state';
  import {
    Decoration,
    type DecorationSet,
    drawSelection,
    dropCursor,
    EditorView,
    highlightActiveLine,
    highlightActiveLineGutter,
    keymap,
    lineNumbers,
    rectangularSelection,
    type ViewUpdate
  } from '@codemirror/view';
  import {
    bracketMatching,
    defaultHighlightStyle,
    foldGutter,
    foldKeymap,
    indentOnInput,
    indentUnit,
    syntaxHighlighting
  } from '@codemirror/language';
  import { defaultKeymap, history, historyKeymap, indentWithTab, isolateHistory } from '@codemirror/commands';
  import { highlightSelectionMatches } from '@codemirror/search';
  import { ChevronDown, ChevronRight, ChevronUp, Replace, ReplaceAll, Search, X } from 'lucide-svelte';
  import { api, type AppSettings } from '../../lib/tauriApi';
  import type { CursorPosition, EditorScrollPosition } from '../../app/stores/documentStore';
  import type { ToolbarAction } from '../../lib/markdownToolbar';
  import { t, translator } from '../../lib/i18n';
  import {
    createEditorSessionSnapshotController,
    createEditorState,
    type SerializedEditorState
  } from '../../lib/editorSession';
  import { startupElapsedMs } from '../../lib/startupLifecycle';
  import { codeMirrorShortcutFor } from '../../lib/commands';
  import { formatCodeBlock, formatInlineCode, formatLines } from '../../lib/editorFormatting';
  import { editorMarkdownLanguage } from '../../lib/editorMarkdownLanguage';
  import {
    editorSearchReplacementChange,
    editorSearchMatchAtPosition,
    findEditorSearchMatches,
    firstEditorSearchMatchAtOrAfter,
    updateEditorSearchMatches,
    visibleEditorSearchMatches,
    type EditorSearchMatch
  } from '../../lib/editorSearch';

  const setSearchDecorations = StateEffect.define<DecorationSet>();
  const searchDecorations = StateField.define<DecorationSet>({
    create: () => Decoration.none,
    update(value, transaction) {
      for (const effect of transaction.effects) {
        if (effect.is(setSearchDecorations)) return effect.value;
      }

      if (transaction.docChanged) return value.map(transaction.changes);
      return value;
    },
    provide: (field) => EditorView.decorations.from(field)
  });

  export let tabId: string;
  export let value = '';
  export let settings: AppSettings;
  export let serializedState: SerializedEditorState | null = null;
  export let initialScrollPosition: EditorScrollPosition;
  export let onChange: (tabId: string, value: string, lineCount: number) => void;
  export let onDirty: (tabId: string) => void;
  export let onCursorChange: (tabId: string, position: CursorPosition) => void;
  export let onScrollSync: (tabId: string, position: EditorScrollPosition, userInitiated: boolean) => void;
  export let onSessionChange: (tabId: string, state: SerializedEditorState) => void;

  let host: HTMLDivElement;
  let view: EditorView | null = null;
  let lastExternalValue = value;
  let lastSettingsSignature = '';
  let searchOpen = false;
  let searchQuery = '';
  let replaceExpanded = false;
  let replaceValue = '';
  let searchInput: HTMLInputElement | null = null;
  let replaceInput: HTMLInputElement | null = null;
  let searchMatches: EditorSearchMatch[] = [];
  let activeMatchIndex = -1;
  let scrollFrame = 0;
  let removeScrollListener: (() => void) | null = null;
  let userEditorScrollUntil = 0;
  let contentSnapshotTimer = 0;
  let pendingDocument: Text | null = null;
  let applyingExternalValue = false;
  let editorMountFailed = false;
  const editorMountStartedAt = performance.now();
  const recordStartupEditorMount = !editorStartupMountReported;
  editorStartupMountReported = true;
  const optionsCompartment = new Compartment();
  const keymapCompartment = new Compartment();
  const sessionSnapshots = createEditorSessionSnapshotController((state) => onSessionChange(tabId, state));

  $: matchCounter = searchQuery ? `${searchMatches.length ? activeMatchIndex + 1 : 0}/${searchMatches.length}` : '0/0';
  $: canReplace = searchQuery.trim().length > 0 && searchMatches.length > 0;

  onMount(() => {
    mountEditor();
  });

  function mountEditor() {
    cleanupEditorView(false);
    recordEditorMount('started');
    try {
      const extensions = [
        history(),
        EditorState.allowMultipleSelections.of(true),
        editorMarkdownLanguage(),
        indentOnInput(),
        bracketMatching(),
        drawSelection(),
        dropCursor(),
        rectangularSelection(),
        highlightSelectionMatches(),
        searchDecorations,
        foldGutter(),
        syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
        keymapCompartment.of(keymap.of(editorKeymap())),
        EditorView.updateListener.of(handleUpdate),
        optionsCompartment.of(optionExtensions())
      ];
      const state = createEditorState(value, extensions, serializedState);
      const createdView = new EditorView({
        state,
        parent: host
      });
      view = createdView;
      createdView.scrollDOM.addEventListener('scroll', scheduleScrollSync, { passive: true });
      const markUserScroll = () => { userEditorScrollUntil = performance.now() + 500; };
      const markDragScroll = (event: PointerEvent) => { if (event.buttons) markUserScroll(); };
      createdView.scrollDOM.addEventListener('wheel', markUserScroll, { passive: true });
      createdView.scrollDOM.addEventListener('pointerdown', markUserScroll, { passive: true });
      createdView.scrollDOM.addEventListener('pointermove', markDragScroll, { passive: true });
      createdView.scrollDOM.addEventListener('touchstart', markUserScroll, { passive: true });
      createdView.scrollDOM.addEventListener('keydown', markUserScroll);
      removeScrollListener = () => {
        createdView.scrollDOM.removeEventListener('scroll', scheduleScrollSync);
        createdView.scrollDOM.removeEventListener('wheel', markUserScroll);
        createdView.scrollDOM.removeEventListener('pointerdown', markUserScroll);
        createdView.scrollDOM.removeEventListener('pointermove', markDragScroll);
        createdView.scrollDOM.removeEventListener('touchstart', markUserScroll);
        createdView.scrollDOM.removeEventListener('keydown', markUserScroll);
      };
      restoreScrollPosition();
      lastSettingsSignature = settingsSignature(settings);
      editorMountFailed = false;
      recordEditorMount('succeeded');
    } catch {
      cleanupEditorView(false);
      editorMountFailed = true;
      recordEditorMount('failed');
    }
  }

  function recordEditorMount(status: 'started' | 'succeeded' | 'failed') {
    if (!recordStartupEditorMount) return;
    void api
      .recordFrontendStartupEvent({
        stage: 'editorMount',
        status,
        code: status === 'failed' ? 'editorMountFailed' : null,
        elapsedMs: startupElapsedMs(editorMountStartedAt)
      })
      .catch(() => undefined);
  }

  function cleanupEditorView(persist: boolean) {
    if (persist && view) {
      flushContent();
      sessionSnapshots.flush(view.state);
      emitScrollPosition(false);
    } else {
      sessionSnapshots.cancel();
    }
    removeScrollListener?.();
    removeScrollListener = null;
    if (scrollFrame) window.cancelAnimationFrame(scrollFrame);
    scrollFrame = 0;
    if (contentSnapshotTimer) window.clearTimeout(contentSnapshotTimer);
    contentSnapshotTimer = 0;
    pendingDocument = null;
    view?.destroy();
    view = null;
  }

  onDestroy(() => {
    cleanupEditorView(true);
  });

  $: if (view && value !== lastExternalValue) {
    if (contentSnapshotTimer) window.clearTimeout(contentSnapshotTimer);
    contentSnapshotTimer = 0;
    pendingDocument = null;
    applyingExternalValue = true;
    const transaction = view.state.update({
      changes: { from: 0, to: view.state.doc.length, insert: value }
    });
    view.dispatch(transaction);
    applyingExternalValue = false;
    lastExternalValue = value;
  }

  $: if (view && settingsSignature(settings) !== lastSettingsSignature) {
    view.dispatch({
      effects: [
        optionsCompartment.reconfigure(optionExtensions()),
        keymapCompartment.reconfigure(keymap.of(editorKeymap()))
      ]
    });
    lastSettingsSignature = settingsSignature(settings);
  }

  function handleUpdate(update: ViewUpdate) {
    if (update.docChanged) {
      if (!applyingExternalValue) {
        onDirty(tabId);
        scheduleContentSnapshot(update.state.doc);
      }

      if (searchOpen) {
        searchMatches = updateEditorSearchMatches(update.state.doc, searchQuery, searchMatches, update.changes);
        refreshSearchState({ keepActiveNearSelection: true, selectActive: false });
      }

      scheduleScrollSync();
    }

    if (update.docChanged || update.selectionSet) {
      const head = update.state.selection.main.head;
      const line = update.state.doc.lineAt(head);
      onCursorChange(tabId, {
        line: line.number,
        column: head - line.from + 1
      });
    }

    if (update.docChanged || update.selectionSet) sessionSnapshots.request(update.state);
    if (searchOpen && update.viewportChanged && !update.docChanged) updateSearchDecorations();
  }

  function scheduleContentSnapshot(document: Text) {
    pendingDocument = document;
    if (contentSnapshotTimer) window.clearTimeout(contentSnapshotTimer);
    contentSnapshotTimer = window.setTimeout(flushContent, 100);
  }

  export function flushContent() {
    if (contentSnapshotTimer) window.clearTimeout(contentSnapshotTimer);
    contentSnapshotTimer = 0;
    const document = pendingDocument;
    pendingDocument = null;
    if (!document) return;
    const next = document.toString();
    lastExternalValue = next;
    onChange(tabId, next, document.lines);
  }

  function scheduleScrollSync() {
    if (!settings.syncScroll) return;
    if (scrollFrame) return;
    scrollFrame = window.requestAnimationFrame(() => {
      scrollFrame = 0;
      emitScrollPosition(true);
    });
  }

  function emitScrollPosition(measureVisibleLine: boolean) {
    if (!view) return;
    const scroller = view.scrollDOM;
    const maxScroll = Math.max(0, scroller.scrollHeight - scroller.clientHeight);
    const ratio = maxScroll > 0 ? scroller.scrollTop / maxScroll : 0;
    const documentY = Math.max(0, scroller.getBoundingClientRect().top - view.documentTop + 1);
    const block = measureVisibleLine ? view.lineBlockAtHeight(documentY) : null;
    const blockProgress = block?.height ? Math.min(1, Math.max(0, (documentY - block.top) / block.height)) : 0;
    const offsetUtf16 = block
      ? Math.min(view.state.doc.length, block.from + Math.round(block.length * blockProgress))
      : view.state.selection.main.head;
    const line = view.state.doc.lineAt(offsetUtf16).number;

    onScrollSync(tabId, {
      line,
      offsetUtf16,
      ratio: Math.min(1, Math.max(0, ratio)),
      totalLines: view.state.doc.lines,
      scrollTop: scroller.scrollTop,
      scrollHeight: scroller.scrollHeight,
      clientHeight: scroller.clientHeight
    }, performance.now() < userEditorScrollUntil);
  }

  function restoreScrollPosition() {
    if (scrollFrame) window.cancelAnimationFrame(scrollFrame);
    scrollFrame = window.requestAnimationFrame(() => {
      scrollFrame = 0;
      if (!view) return;
      const scroller = view.scrollDOM;
      const maxScroll = Math.max(0, scroller.scrollHeight - scroller.clientHeight);
      const ratio = Math.min(1, Math.max(0, initialScrollPosition?.ratio ?? 0));
      scroller.scrollTop = maxScroll > 0 ? ratio * maxScroll : Math.max(0, initialScrollPosition?.scrollTop ?? 0);
      if (settings.syncScroll) emitScrollPosition(true);
    });
  }

  function optionExtensions() {
    return [
      ...(settings.showLineNumbers ? [lineNumbers(), highlightActiveLineGutter()] : []),
      highlightActiveLine(),
      ...(settings.wordWrap ? [EditorView.lineWrapping] : []),
      EditorState.tabSize.of(settings.tabSize),
      indentUnit.of(settings.insertSpaces ? ' '.repeat(settings.tabSize) : '\t'),
      EditorView.theme({
        '&': {
          height: '100%',
          color: 'var(--text-primary)',
          background: 'var(--editor-bg)',
          fontSize: `${settings.editorFontSize}px`,
          fontFamily: settings.editorFontFamily
        },
        '.cm-scroller': {
          lineHeight: String(settings.lineHeight),
          overflow: 'auto'
        },
        '.cm-content': {
          padding: '18px 20px',
          caretColor: 'var(--editor-caret-color)',
          minHeight: '100%'
        },
        '.cm-cursor, .cm-dropCursor': {
          borderLeft: '2.5px solid var(--editor-caret-color) !important',
          boxShadow: '0 0 7px var(--editor-caret-glow)',
          marginLeft: '-1px'
        },
        '.cm-gutters': {
          background: 'var(--editor-bg)',
          color: 'var(--text-muted)',
          borderRight: '1px solid var(--border-color)'
        },
        '.cm-activeLine': {
          backgroundColor: 'color-mix(in srgb, var(--active-line-bg) 62%, transparent)'
        },
        '.cm-activeLineGutter': {
          backgroundColor: 'color-mix(in srgb, var(--active-line-bg) 62%, transparent)',
          color: 'var(--text-primary)'
        },
        '.cm-selectionLayer .cm-selectionBackground, &.cm-focused .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground, &.cm-focused .cm-selectionBackground': {
          backgroundColor: 'color-mix(in srgb, var(--selection-bg) 88%, transparent) !important'
        },
        '.cm-content ::selection': {
          backgroundColor: 'color-mix(in srgb, var(--selection-bg) 88%, transparent)'
        },
        '&.cm-focused': {
          outline: 'none'
        },
        '.cm-searchMatch': {
          backgroundColor: 'color-mix(in srgb, var(--accent-color) 22%, transparent)',
          outline: '1px solid color-mix(in srgb, var(--accent-color) 28%, transparent)'
        },
        '.cm-searchMatch-active': {
          backgroundColor: 'color-mix(in srgb, var(--accent-color) 42%, transparent)',
          outline: '1px solid var(--accent-color)'
        }
      })
    ];
  }

  function editorKeymap() {
    return [
      { key: codeMirrorShortcutFor('format-bold'), run: () => runToolbar('bold') },
      { key: codeMirrorShortcutFor('format-italic'), run: () => runToolbar('italic') },
      { key: codeMirrorShortcutFor('insert-link'), run: () => runToolbar('link') },
      { key: codeMirrorShortcutFor('find'), run: () => openFindFromKeymap() },
      { key: codeMirrorShortcutFor('replace'), run: () => openReplaceFromKeymap() },
      ...defaultKeymap,
      ...historyKeymap,
      ...foldKeymap,
      ...(settings.insertSpaces ? [] : [indentWithTab])
    ];
  }

  function settingsSignature(next: AppSettings): string {
    return [
      next.showLineNumbers,
      next.wordWrap,
      next.tabSize,
      next.insertSpaces,
      next.editorFontFamily,
      next.editorFontSize,
      next.lineHeight
    ].join('|');
  }

  function runToolbar(action: ToolbarAction): boolean {
    applyMarkdown(action);
    return true;
  }

  export function applyMarkdown(action: ToolbarAction) {
    if (!view) return;

    switch (action) {
      case 'bold':
        wrapSelection('**', '**', t('editor.insert.bold'));
        break;
      case 'italic':
        wrapSelection('*', '*', t('editor.insert.italic'));
        break;
      case 'strike':
        wrapSelection('~~', '~~', t('editor.insert.strike'));
        break;
      case 'inlineCode':
        dispatchFormatting(formatInlineCode(view.state));
        break;
      case 'h1':
      case 'h2':
      case 'h3':
      case 'quote':
      case 'unorderedList':
      case 'orderedList':
      case 'taskList':
        dispatchFormatting(formatLines(view.state, action));
        break;
      case 'link':
        wrapSelection('[', '](https://example.com)', t('editor.insert.link'));
        break;
      case 'image':
        insertBlock(t('editor.insert.image'));
        break;
      case 'codeBlock':
        dispatchFormatting(formatCodeBlock(view.state));
        break;
      case 'table':
        insertBlock(t('editor.insert.table'));
        break;
      case 'hr':
        insertBlock('---');
        break;
    }
  }

  export function openFind() {
    if (!view) return;
    const selected = selectedSearchText();
    if (selected) {
      searchQuery = selected;
    }
    searchOpen = true;
    refreshSearchMatches({ selectActive: true });

    void tick().then(() => {
      searchInput?.focus();
      searchInput?.select();
    });
  }

  export function openReplace() {
    openFind();
    replaceExpanded = true;
  }

  function openFindFromKeymap(): boolean {
    openFind();
    return true;
  }

  function openReplaceFromKeymap(): boolean {
    openReplace();
    return true;
  }

  function closeFind() {
    searchOpen = false;
    searchMatches = [];
    activeMatchIndex = -1;
    updateSearchDecorations();
    view?.focus();
  }

  function toggleReplaceExpanded() {
    replaceExpanded = !replaceExpanded;
    if (replaceExpanded) {
      void tick().then(() => {
        replaceInput?.focus();
        replaceInput?.select();
      });
    } else {
      void tick().then(() => searchInput?.focus());
    }
  }

  function selectedSearchText(): string {
    if (!view) return '';
    const selection = view.state.selection.main;
    if (selection.empty) return '';

    const selected = view.state.sliceDoc(selection.from, selection.to).trim();
    if (!selected || selected.includes('\n')) return '';
    return selected;
  }

  function handleSearchInput(event: Event) {
    searchQuery = (event.currentTarget as HTMLInputElement).value;
    activeMatchIndex = searchQuery ? 0 : -1;
    refreshSearchMatches({ selectActive: true });
  }

  function handleSearchKeydown(event: KeyboardEvent) {
    if (event.key === 'Enter') {
      event.preventDefault();
      if (event.shiftKey) {
        moveSearchMatch(-1);
      } else {
        moveSearchMatch(1);
      }
    } else if (event.key === 'Escape') {
      event.preventDefault();
      closeFind();
    }
  }

  function handleReplaceInput(event: Event) {
    replaceValue = (event.currentTarget as HTMLInputElement).value;
  }

  function handleReplaceKeydown(event: KeyboardEvent) {
    if (event.key === 'Enter') {
      event.preventDefault();
      if (event.ctrlKey || event.metaKey) {
        replaceAllMatches();
      } else {
        replaceCurrentMatch();
      }
    } else if (event.key === 'Escape') {
      event.preventDefault();
      closeFind();
    }
  }

  function moveSearchMatch(direction: 1 | -1) {
    if (!searchMatches.length) return;
    activeMatchIndex = (activeMatchIndex + direction + searchMatches.length) % searchMatches.length;
    selectActiveMatch();
  }

  function replaceCurrentMatch() {
    if (!view || !canReplace) return;

    const match = searchMatches[activeMatchIndex];
    if (!match) return;

    const nextSearchAnchor = match.from + replaceValue.length;
    view.dispatch({
      changes: { from: match.from, to: match.to, insert: replaceValue },
      selection: EditorSelection.cursor(nextSearchAnchor)
    });

    if (!searchMatches.length) {
      activeMatchIndex = -1;
      updateSearchDecorations();
      void tick().then(() => replaceInput?.focus());
      return;
    }

    activeMatchIndex = firstMatchIndexAtOrAfter(nextSearchAnchor);
    selectActiveMatch('replace');
  }

  function replaceAllMatches() {
    if (!view || !canReplace) return;

    const anchor = searchMatches[0]?.from ?? 0;
    const replacementChange = editorSearchReplacementChange(view.state.doc, searchMatches, replaceValue);
    if (!replacementChange) return;
    view.dispatch({
      changes: replacementChange,
      selection: EditorSelection.cursor(anchor + replaceValue.length)
    });

    activeMatchIndex = searchMatches.length ? firstMatchIndexAtOrAfter(anchor + replaceValue.length) : -1;

    if (searchMatches.length) {
      selectActiveMatch('replace');
    } else {
      updateSearchDecorations();
      void tick().then(() => replaceInput?.focus());
    }
  }

  function refreshSearchMatches(options: { keepActiveNearSelection?: boolean; selectActive?: boolean } = {}) {
    if (!view) return;
    searchMatches = findEditorSearchMatches(view.state.doc, searchQuery);
    refreshSearchState(options);
  }

  function refreshSearchState(options: { keepActiveNearSelection?: boolean; selectActive?: boolean } = {}) {
    if (!view) return;

    if (!searchMatches.length) {
      activeMatchIndex = -1;
      updateSearchDecorations();
      return;
    }

    if (options.keepActiveNearSelection) {
      const head = view.state.selection.main.head;
      const nearIndex = editorSearchMatchAtPosition(searchMatches, head);
      if (nearIndex >= 0) {
        activeMatchIndex = nearIndex;
      } else if (activeMatchIndex >= searchMatches.length || activeMatchIndex < 0) {
        activeMatchIndex = 0;
      }
    } else if (activeMatchIndex >= searchMatches.length || activeMatchIndex < 0) {
      activeMatchIndex = 0;
    }

    if (options.selectActive) {
      selectActiveMatch();
      return;
    }

    updateSearchDecorations();
  }

  function firstMatchIndexAtOrAfter(position: number): number {
    return firstEditorSearchMatchAtOrAfter(searchMatches, position);
  }

  function selectActiveMatch(focusTarget: 'search' | 'replace' = 'search') {
    if (!view) return;
    const match = searchMatches[activeMatchIndex];
    if (!match) {
      updateSearchDecorations();
      return;
    }

    view.dispatch({
      selection: EditorSelection.range(match.from, match.to),
      effects: [
        setSearchDecorations.of(buildSearchDecorations()),
        EditorView.scrollIntoView(match.from, { y: 'center' })
      ]
    });

    window.requestAnimationFrame(() => {
      if (focusTarget === 'replace') {
        replaceInput?.focus();
      } else {
        searchInput?.focus();
      }
    });
  }

  function updateSearchDecorations() {
    view?.dispatch({
      effects: setSearchDecorations.of(buildSearchDecorations())
    });
  }

  function buildSearchDecorations(): DecorationSet {
    if (!searchMatches.length) return Decoration.none;

    const builder = new RangeSetBuilder<Decoration>();
    for (const { index, match } of visibleEditorSearchMatches(
      searchMatches,
      view?.visibleRanges ?? [],
      activeMatchIndex
    )) {
      builder.add(
        match.from,
        match.to,
        Decoration.mark({
          class: index === activeMatchIndex ? 'cm-searchMatch cm-searchMatch-active' : 'cm-searchMatch'
        })
      );
    }
    return builder.finish();
  }

  export function jumpToLine(lineNumber: number) {
    if (!view) return;
    const line = view.state.doc.line(Math.min(Math.max(1, lineNumber), view.state.doc.lines));
    view.dispatch({
      selection: EditorSelection.cursor(line.from),
      effects: EditorView.scrollIntoView(line.from, { y: 'center' })
    });
    view.focus();
  }

  export function scrollToLine(lineNumber: number) {
    if (!view) return;
    userEditorScrollUntil = 0;
    const line = view.state.doc.line(Math.min(Math.max(1, lineNumber), view.state.doc.lines));
    view.dispatch({ effects: EditorView.scrollIntoView(line.from, { y: 'start' }) });
  }

  function wrapSelection(prefix: string, suffix: string, placeholder: string) {
    if (!view) return;
    const selection = view.state.selection.main;
    const selected = view.state.sliceDoc(selection.from, selection.to);
    const body = selected || placeholder;
    const insert = `${prefix}${body}${suffix}`;
    const anchor = selection.from + prefix.length;
    const head = anchor + body.length;

    view.dispatch({
      changes: { from: selection.from, to: selection.to, insert },
      selection: EditorSelection.range(anchor, head),
      annotations: isolateHistory.of('full')
    });
    view.focus();
  }

  function dispatchFormatting(spec: Parameters<EditorView['dispatch']>[0] | null) {
    if (!view || !spec) return;
    view.dispatch({ ...spec, annotations: isolateHistory.of('full') });
    view.focus();
  }

  function insertBlock(block: string) {
    if (!view) return;
    const selection = view.state.selection.main;
    const before = selection.from > 0 ? '\n\n' : '';
    const after = selection.to < view.state.doc.length ? '\n\n' : '';
    const insert = `${before}${block}${after}`;

    view.dispatch({
      changes: { from: selection.from, to: selection.to, insert },
      selection: EditorSelection.cursor(selection.from + before.length + block.length),
      annotations: isolateHistory.of('full')
    });
    view.focus();
  }
</script>

<div class="editor-wrap">
  <div
    class="editor-host"
    bind:this={host}
    data-marklite-application-shortcuts="true"
  ></div>

  {#if editorMountFailed}
    <section class="editor-startup-error" role="alert">
      <strong>{$translator('editor.loadFailed')}</strong>
      <span>{$translator('editor.loadFailedDescription')}</span>
      <button type="button" on:click={mountEditor}>{$translator('editor.retry')}</button>
    </section>
  {/if}

  {#if searchOpen}
    <form class="find-popover" class:replace-expanded={replaceExpanded} role="search" on:submit|preventDefault={() => moveSearchMatch(1)}>
      <div class="find-row">
        <button
          type="button"
          class="find-icon-button replace-toggle"
          class:expanded={replaceExpanded}
          title={replaceExpanded ? $translator('editor.find.collapseReplace') : $translator('editor.find.expandReplace')}
          aria-label={replaceExpanded ? $translator('editor.find.collapseReplace') : $translator('editor.find.expandReplace')}
          aria-expanded={replaceExpanded}
          on:click={toggleReplaceExpanded}
        >
          <span class="replace-toggle-icon">
            <ChevronRight size={16} />
          </span>
        </button>

        <label class="find-input-shell" aria-label={$translator('editor.find.search')}>
          <Search size={15} />
          <input
            bind:this={searchInput}
            value={searchQuery}
            placeholder={$translator('editor.find.search')}
            spellcheck="false"
            on:input={handleSearchInput}
            on:keydown={handleSearchKeydown}
          />
          <span class:empty={!searchMatches.length}>{matchCounter}</span>
        </label>

        <button type="button" class="find-icon-button" title={$translator('editor.find.previous')} disabled={!searchMatches.length} on:click={() => moveSearchMatch(-1)}>
          <ChevronUp size={16} />
        </button>
        <button type="button" class="find-icon-button" title={$translator('editor.find.next')} disabled={!searchMatches.length} on:click={() => moveSearchMatch(1)}>
          <ChevronDown size={16} />
        </button>
        <button type="button" class="find-icon-button" title={$translator('editor.find.close')} on:click={closeFind}>
          <X size={16} />
        </button>
      </div>

      {#if replaceExpanded}
        <div class="replace-row">
          <span class="replace-row-spacer" aria-hidden="true"></span>
          <label class="replace-input-shell" aria-label={$translator('editor.find.replace')}>
            <Replace size={15} />
            <input
              bind:this={replaceInput}
              value={replaceValue}
              placeholder={$translator('editor.find.replace')}
              spellcheck="false"
              on:input={handleReplaceInput}
              on:keydown={handleReplaceKeydown}
            />
          </label>
          <button type="button" class="find-icon-button" title={$translator('editor.find.replaceOne')} disabled={!canReplace} on:click={replaceCurrentMatch}>
            <Replace size={16} />
          </button>
          <button type="button" class="find-icon-button" title={$translator('editor.find.replaceAll')} disabled={!canReplace} on:click={replaceAllMatches}>
            <ReplaceAll size={16} />
          </button>
          <span class="replace-row-spacer" aria-hidden="true"></span>
        </div>
      {/if}
    </form>
  {/if}
</div>

<style>
  .editor-wrap {
    position: relative;
    width: 100%;
    height: 100%;
  }

  .editor-startup-error {
    position: absolute;
    inset: 0;
    display: grid;
    place-content: center;
    justify-items: center;
    gap: 10px;
    padding: 24px;
    background: var(--bg-primary);
    color: var(--text-secondary);
    text-align: center;
  }

  .editor-startup-error strong {
    color: var(--text-primary);
    font-size: 1rem;
  }

  .editor-startup-error button {
    padding: 8px 12px;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-md);
    background: var(--panel-bg);
    color: var(--text-primary);
    cursor: pointer;
  }

  .find-popover {
    position: fixed;
    top: 144px;
    right: 18px;
    z-index: 20;
    display: grid;
    gap: 6px;
    width: min(540px, calc(100% - 36px));
    padding: 6px;
    border: 1px solid var(--border-color);
    border-radius: calc(var(--radius-md) + 2px);
    background: color-mix(in srgb, var(--panel-bg) 94%, transparent);
    box-shadow: var(--shadow-soft);
    backdrop-filter: blur(16px);
  }

  .find-row,
  .replace-row {
    display: grid;
    grid-template-columns: 30px minmax(180px, 1fr) 32px 32px 32px;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }

  .find-input-shell {
    display: grid;
    grid-template-columns: 18px minmax(110px, 1fr) auto;
    align-items: center;
    gap: 7px;
    height: 32px;
    min-width: 0;
    padding: 0 9px;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-md);
    background: var(--input-bg);
    color: var(--text-muted);
  }

  .replace-input-shell {
    display: grid;
    grid-template-columns: 18px minmax(110px, 1fr);
    align-items: center;
    gap: 7px;
    height: 32px;
    min-width: 0;
    padding: 0 9px;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-md);
    background: var(--input-bg);
    color: var(--text-muted);
  }

  .find-input-shell input {
    min-width: 0;
    width: 100%;
    border: 0;
    outline: 0;
    background: transparent;
    color: var(--text-primary);
    font-size: 0.86rem;
  }

  .replace-input-shell input {
    min-width: 0;
    width: 100%;
    border: 0;
    outline: 0;
    background: transparent;
    color: var(--text-primary);
    font-size: 0.86rem;
  }

  .find-input-shell span {
    min-width: 42px;
    color: var(--text-secondary);
    font-size: 0.75rem;
    text-align: right;
  }

  .find-input-shell span.empty {
    color: var(--text-muted);
  }

  .find-icon-button {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border: 1px solid transparent;
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--text-secondary);
    cursor: pointer;
  }

  .replace-toggle-icon {
    display: grid;
    place-items: center;
    transition: transform 130ms ease;
  }

  .replace-toggle.expanded .replace-toggle-icon {
    transform: rotate(90deg);
  }

  .replace-row-spacer {
    display: block;
    width: 30px;
    height: 1px;
  }

  .find-icon-button:hover:not(:disabled) {
    background: var(--bg-secondary);
    color: var(--text-primary);
  }

  .find-icon-button:disabled {
    cursor: default;
    opacity: 0.38;
  }

  @media (max-width: 680px) {
    .find-popover {
      left: 12px;
      right: 12px;
      width: auto;
    }

    .find-row,
    .replace-row {
      grid-template-columns: 28px minmax(0, 1fr) 30px 30px 30px;
    }
  }
</style>
