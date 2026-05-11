<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { EditorSelection, EditorState, Compartment } from '@codemirror/state';
  import {
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
  import { highlightSelectionMatches, openSearchPanel, searchKeymap } from '@codemirror/search';
  import type { AppSettings } from '../../lib/tauriApi';
  import type { CursorPosition } from '../../app/stores/documentStore';
  import type { ToolbarAction } from '../../lib/markdownToolbar';

  export let value = '';
  export let settings: AppSettings;
  export let onChange: (value: string) => void = () => {};
  export let onCursorChange: (position: CursorPosition) => void = () => {};

  let host: HTMLDivElement;
  let view: EditorView | null = null;
  let lastExternalValue = value;
  let lastSettingsSignature = '';
  const optionsCompartment = new Compartment();

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
        foldGutter(),
        syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
        keymap.of([
          { key: 'Mod-b', run: () => runToolbar('bold') },
          { key: 'Mod-i', run: () => runToolbar('italic') },
          { key: 'Mod-k', run: () => runToolbar('link') },
          { key: 'Mod-h', run: openSearchPanel },
          ...defaultKeymap,
          ...historyKeymap,
          ...foldKeymap,
          ...searchKeymap,
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
        '.cm-selectionBackground, &.cm-focused .cm-selectionBackground': {
          backgroundColor: 'var(--selection-bg)'
        },
        '&.cm-focused': {
          outline: 'none'
        },
        '.cm-search': {
          background: 'var(--panel-bg)',
          color: 'var(--text-primary)',
          borderTop: '1px solid var(--border-color)',
          padding: '8px'
        },
        '.cm-search input': {
          background: 'var(--input-bg)',
          color: 'var(--text-primary)',
          border: '1px solid var(--border-color)',
          borderRadius: '6px'
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
    openSearchPanel(view);
    view.focus();
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

<div class="editor-host" bind:this={host}></div>
