<script lang="ts">
  import { onDestroy, onMount, tick } from 'svelte';
  import { Compartment, EditorSelection, EditorState, RangeSetBuilder, StateEffect, StateField } from '@codemirror/state';
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
  import { markdown } from '@codemirror/lang-markdown';
  import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
  import { highlightSelectionMatches } from '@codemirror/search';
  import { ChevronDown, ChevronUp, Search, X } from 'lucide-svelte';
  import type { AppSettings } from '../../lib/tauriApi';
  import type { CursorPosition } from '../../app/stores/documentStore';
  import type { ToolbarAction } from '../../lib/markdownToolbar';

  type SearchMatch = {
    from: number;
    to: number;
  };

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

  export let value = '';
  export let settings: AppSettings;
  export let onChange: (value: string) => void = () => {};
  export let onCursorChange: (position: CursorPosition) => void = () => {};

  let host: HTMLDivElement;
  let view: EditorView | null = null;
  let lastExternalValue = value;
  let lastSettingsSignature = '';
  let searchOpen = false;
  let searchQuery = '';
  let searchInput: HTMLInputElement | null = null;
  let searchMatches: SearchMatch[] = [];
  let activeMatchIndex = -1;
  const optionsCompartment = new Compartment();

  $: matchCounter = searchQuery ? `${searchMatches.length ? activeMatchIndex + 1 : 0}/${searchMatches.length}` : '0/0';

  onMount(() => {
    const state = EditorState.create({
      doc: value,
      extensions: [
        history(),
        markdown(),
        indentOnInput(),
        bracketMatching(),
        drawSelection(),
        dropCursor(),
        rectangularSelection(),
        highlightSelectionMatches(),
        searchDecorations,
        foldGutter(),
        syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
        keymap.of([
          { key: 'Mod-b', run: () => runToolbar('bold') },
          { key: 'Mod-i', run: () => runToolbar('italic') },
          { key: 'Mod-k', run: () => runToolbar('link') },
          { key: 'Mod-f', run: () => openFindFromKeymap() },
          { key: 'Mod-h', run: () => openFindFromKeymap() },
          ...defaultKeymap,
          ...historyKeymap,
          ...foldKeymap,
          ...(settings.insertSpaces ? [] : [indentWithTab])
        ]),
        EditorView.updateListener.of(handleUpdate),
        optionsCompartment.of(optionExtensions())
      ]
    });

    view = new EditorView({
      state,
      parent: host
    });
    lastSettingsSignature = settingsSignature(settings);
  });

  onDestroy(() => {
    view?.destroy();
    view = null;
  });

  $: if (view && value !== lastExternalValue && value !== view.state.doc.toString()) {
    const transaction = view.state.update({
      changes: { from: 0, to: view.state.doc.length, insert: value }
    });
    view.dispatch(transaction);
    lastExternalValue = value;
  }

  $: if (view && settingsSignature(settings) !== lastSettingsSignature) {
    view.dispatch({
      effects: optionsCompartment.reconfigure(optionExtensions())
    });
    lastSettingsSignature = settingsSignature(settings);
  }

  function handleUpdate(update: ViewUpdate) {
    if (update.docChanged) {
      const next = update.state.doc.toString();
      lastExternalValue = next;
      onChange(next);

      if (searchOpen) {
        refreshSearchMatches({ keepActiveNearSelection: true, selectActive: false });
      }
    }

    if (update.docChanged || update.selectionSet) {
      const head = update.state.selection.main.head;
      const line = update.state.doc.lineAt(head);
      onCursorChange({
        line: line.number,
        column: head - line.from + 1
      });
    }
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
          caretColor: 'var(--accent-color)',
          minHeight: '100%'
        },
        '.cm-gutters': {
          background: 'var(--editor-bg)',
          color: 'var(--text-muted)',
          borderRight: '1px solid var(--border-color)'
        },
        '.cm-activeLine': {
          backgroundColor: 'var(--active-line-bg)'
        },
        '.cm-activeLineGutter': {
          backgroundColor: 'var(--active-line-bg)',
          color: 'var(--text-primary)'
        },
        '.cm-selectionLayer .cm-selectionBackground, &.cm-focused .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground, &.cm-focused .cm-selectionBackground': {
          backgroundColor: 'var(--selection-bg) !important'
        },
        '.cm-content ::selection': {
          backgroundColor: 'var(--selection-bg)'
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
        wrapSelection('**', '**', '加粗文本');
        break;
      case 'italic':
        wrapSelection('*', '*', '斜体文本');
        break;
      case 'strike':
        wrapSelection('~~', '~~', '删除线文本');
        break;
      case 'inlineCode':
        wrapSelection('`', '`', 'code');
        break;
      case 'h1':
        prefixCurrentLine('# ');
        break;
      case 'h2':
        prefixCurrentLine('## ');
        break;
      case 'h3':
        prefixCurrentLine('### ');
        break;
      case 'quote':
        prefixCurrentLine('> ');
        break;
      case 'unorderedList':
        prefixCurrentLine('- ');
        break;
      case 'orderedList':
        prefixCurrentLine('1. ');
        break;
      case 'taskList':
        prefixCurrentLine('- [ ] ');
        break;
      case 'link':
        wrapSelection('[', '](https://example.com)', '链接文本');
        break;
      case 'image':
        insertBlock('![图片描述](./image.png)');
        break;
      case 'codeBlock':
        wrapBlock('```\n', '\n```', 'code');
        break;
      case 'table':
        insertBlock('| 列 A | 列 B |\n| --- | --- |\n| 内容 | 内容 |');
        break;
      case 'hr':
        insertBlock('---');
        break;
    }
  }

  export function focusEditor() {
    view?.focus();
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

  function openFindFromKeymap(): boolean {
    openFind();
    return true;
  }

  function closeFind() {
    searchOpen = false;
    searchMatches = [];
    activeMatchIndex = -1;
    updateSearchDecorations();
    view?.focus();
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

  function moveSearchMatch(direction: 1 | -1) {
    if (!searchMatches.length) return;
    activeMatchIndex = (activeMatchIndex + direction + searchMatches.length) % searchMatches.length;
    selectActiveMatch();
  }

  function refreshSearchMatches(options: { keepActiveNearSelection?: boolean; selectActive?: boolean } = {}) {
    if (!view) return;
    searchMatches = findSearchMatches(view.state.doc.toString(), searchQuery);

    if (!searchMatches.length) {
      activeMatchIndex = -1;
      updateSearchDecorations();
      return;
    }

    if (options.keepActiveNearSelection) {
      const head = view.state.selection.main.head;
      const nearIndex = searchMatches.findIndex((match) => match.from <= head && head <= match.to);
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

  function findSearchMatches(content: string, query: string): SearchMatch[] {
    const needle = query.trim();
    if (!needle) return [];

    const matches: SearchMatch[] = [];
    const haystack = content.toLocaleLowerCase();
    const normalizedNeedle = needle.toLocaleLowerCase();
    let from = 0;

    while (from <= haystack.length) {
      const index = haystack.indexOf(normalizedNeedle, from);
      if (index < 0) break;
      matches.push({ from: index, to: index + needle.length });
      from = index + Math.max(needle.length, 1);
    }

    return matches;
  }

  function selectActiveMatch() {
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
      searchInput?.focus();
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
    for (const [index, match] of searchMatches.entries()) {
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
      selection: EditorSelection.range(anchor, head)
    });
    view.focus();
  }

  function wrapBlock(prefix: string, suffix: string, placeholder: string) {
    if (!view) return;
    const selection = view.state.selection.main;
    const selected = view.state.sliceDoc(selection.from, selection.to);
    const body = selected || placeholder;
    const insert = `${prefix}${body}${suffix}`;
    const anchor = selection.from + prefix.length;
    const head = anchor + body.length;

    view.dispatch({
      changes: { from: selection.from, to: selection.to, insert },
      selection: EditorSelection.range(anchor, head)
    });
    view.focus();
  }

  function prefixCurrentLine(prefix: string) {
    if (!view) return;
    const head = view.state.selection.main.head;
    const line = view.state.doc.lineAt(head);
    const current = view.state.sliceDoc(line.from, line.to);
    const cleaned = current.replace(/^#{1,6}\s+|^>\s+|^[-*]\s+|^\d+\.\s+|^- \[[ xX]\]\s+/, '');
    const insert = `${prefix}${cleaned}`;

    view.dispatch({
      changes: { from: line.from, to: line.to, insert },
      selection: EditorSelection.cursor(line.from + insert.length)
    });
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
      selection: EditorSelection.cursor(selection.from + before.length + block.length)
    });
    view.focus();
  }
</script>

<div class="editor-wrap">
  <div class="editor-host" bind:this={host}></div>

  {#if searchOpen}
    <form class="find-popover" role="search" on:submit|preventDefault={() => moveSearchMatch(1)}>
      <label class="find-input-shell" aria-label="搜索文档">
        <Search size={15} />
        <input
          bind:this={searchInput}
          value={searchQuery}
          placeholder="搜索文档"
          spellcheck="false"
          on:input={handleSearchInput}
          on:keydown={handleSearchKeydown}
        />
        <span class:empty={!searchMatches.length}>{matchCounter}</span>
      </label>

      <button type="button" class="find-icon-button" title="上一个" disabled={!searchMatches.length} on:click={() => moveSearchMatch(-1)}>
        <ChevronUp size={16} />
      </button>
      <button type="button" class="find-icon-button" title="下一个" disabled={!searchMatches.length} on:click={() => moveSearchMatch(1)}>
        <ChevronDown size={16} />
      </button>
      <button type="button" class="find-icon-button" title="关闭" on:click={closeFind}>
        <X size={16} />
      </button>
    </form>
  {/if}
</div>

<style>
  .editor-wrap {
    position: relative;
    width: 100%;
    height: 100%;
  }

  .find-popover {
    position: fixed;
    top: 144px;
    right: 18px;
    z-index: 20;
    display: flex;
    align-items: center;
    gap: 6px;
    max-width: min(520px, calc(100% - 32px));
    padding: 6px;
    border: 1px solid var(--border-color);
    border-radius: calc(var(--radius-md) + 2px);
    background: color-mix(in srgb, var(--panel-bg) 94%, transparent);
    box-shadow: var(--shadow-soft);
    backdrop-filter: blur(16px);
  }

  .find-input-shell {
    display: grid;
    grid-template-columns: 18px minmax(110px, 240px) auto;
    align-items: center;
    gap: 7px;
    height: 32px;
    min-width: min(310px, calc(100vw - 190px));
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
      max-width: none;
    }

    .find-input-shell {
      min-width: 0;
      flex: 1;
    }
  }
</style>
