import { createClassComponent } from 'svelte/legacy';
import { flushSync, tick, type SvelteComponent } from 'svelte';
import { EditorSelection } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { undo } from '@codemirror/commands';
import { afterEach, describe, expect, test, vi } from 'vitest';
import MarkdownEditor from './MarkdownEditor.svelte';
import { defaultSettings, type AppSettings } from '../../lib/tauriApi';

const mocks = vi.hoisted(() => ({
  recordFrontendStartupEvent: vi.fn(async () => undefined)
}));

vi.mock('../../lib/tauriApi', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../lib/tauriApi')>();
  return {
    ...actual,
    api: {
      ...actual.api,
      recordFrontendStartupEvent: mocks.recordFrontendStartupEvent
    }
  };
});

let target: HTMLDivElement;
let component: (SvelteComponent & { $set(props: Record<string, unknown>): void; $destroy(): void }) | undefined;

function editorSettings(overrides: Partial<AppSettings> = {}): AppSettings {
  return { ...defaultSettings, ...overrides };
}

async function renderEditor(overrides: Partial<AppSettings> = {}, initialValue = '') {
  const onChange = vi.fn();
  const onDirty = vi.fn();
  const onCursorChange = vi.fn();
  const onScrollSync = vi.fn();
  const onSessionChange = vi.fn();
  target = document.createElement('div');
  document.body.append(target);
  component = createClassComponent({
    component: MarkdownEditor,
    target,
    props: {
      tabId: 'tab-1',
      value: initialValue,
      settings: editorSettings(overrides),
      serializedState: null,
      initialScrollPosition: {
        line: 1,
        ratio: 0,
        totalLines: 1,
        scrollTop: 0,
        scrollHeight: 0,
        clientHeight: 0
      },
      onChange,
      onDirty,
      onCursorChange,
      onScrollSync,
      onSessionChange
    }
  }) as typeof component;
  await tick();
  return { onChange, onDirty, onScrollSync, onSessionChange };
}

function pressTab() {
  const content = target.querySelector<HTMLElement>('.cm-content');
  expect(content).not.toBeNull();
  const handled = !content!.dispatchEvent(
    new KeyboardEvent('keydown', { key: 'Tab', code: 'Tab', bubbles: true, cancelable: true })
  );
  return handled;
}

function editorView(): EditorView {
  const editor = target.querySelector<HTMLElement>('.cm-editor');
  const view = editor && EditorView.findFromDOM(editor);
  expect(view).not.toBeNull();
  return view!;
}

function applyFormatting(action: 'h1' | 'unorderedList' | 'orderedList' | 'taskList' | 'inlineCode' | 'codeBlock') {
  (component as SvelteComponent & { applyMarkdown(action: string): void }).applyMarkdown(action);
}

afterEach(() => {
  component?.$destroy();
  component = undefined;
  target?.remove();
});

describe('MarkdownEditor session and runtime configuration', () => {
  test('converts task lines without leaving a checkbox marker and toggles repeated format', async () => {
    await renderEditor({}, '- [x] 中文 task');
    const view = editorView();
    view.dispatch({ selection: EditorSelection.cursor(view.state.doc.length) });
    applyFormatting('h1');
    expect(view.state.doc.toString()).toBe('# 中文 task');
    applyFormatting('h1');
    expect(view.state.doc.toString()).toBe('中文 task');
    applyFormatting('orderedList');
    expect(view.state.doc.toString()).toBe('1. 中文 task');
    applyFormatting('unorderedList');
    expect(view.state.doc.toString()).toBe('- 中文 task');
    expect(undo(view)).toBe(true);
    expect(view.state.doc.toString()).toBe('1. 中文 task');
  });

  test('chooses inline code delimiters beyond selected backtick runs', async () => {
    await renderEditor({}, 'a``b');
    const view = editorView();
    view.dispatch({ selection: EditorSelection.range(0, view.state.doc.length) });
    applyFormatting('inlineCode');
    expect(view.state.doc.toString()).toBe('```a``b```');
    applyFormatting('inlineCode');
    expect(view.state.doc.toString()).toBe('a``b');
  });

  test('pads inline code containing edge backticks and keeps empty lines during list conversion', async () => {
    await renderEditor({}, '`edge`\n\n- [x] 中文\n2. done');
    const view = editorView();
    view.dispatch({ selection: EditorSelection.range(0, 6) });
    applyFormatting('inlineCode');
    expect(view.state.doc.toString().startsWith('`` `edge` ``')).toBe(true);
    applyFormatting('inlineCode');
    expect(view.state.doc.toString().startsWith('`edge`\n')).toBe(true);

    view.dispatch({ selection: EditorSelection.range(0, view.state.doc.length) });
    applyFormatting('taskList');
    expect(view.state.doc.toString()).toBe('- [ ] `edge`\n\n中文\n- [ ] done');
  });

  test('keeps a selected line range selected through a list conversion', async () => {
    await renderEditor({}, '- [x] one\n\ntwo');
    const view = editorView();
    view.dispatch({ selection: EditorSelection.range(0, view.state.doc.length) });
    applyFormatting('orderedList');
    expect(view.state.doc.toString()).toBe('1. one\n\n1. two');
    expect(view.state.selection.main.from).toBe(0);
    expect(view.state.selection.main.to).toBe(view.state.doc.length);
  });

  test('formats mixed single-line and multi-line code selections independently', async () => {
    await renderEditor({}, 'one and alpha\nbeta');
    const view = editorView();
    view.dispatch({ selection: EditorSelection.create([
      EditorSelection.range(0, 3),
      EditorSelection.range(8, view.state.doc.length)
    ]) });
    applyFormatting('inlineCode');
    expect(view.state.doc.toString()).toBe('`one` and \n\n```\nalpha\nbeta\n```');
    expect(undo(view)).toBe(true);
    expect(view.state.doc.toString()).toBe('one and alpha\nbeta');
  });

  test('fences a paragraph selection with safe block boundaries and preserves source', async () => {
    await renderEditor({}, 'before 中文\r\ncode ``` here\r\nafter');
    const view = editorView();
    const before = view.state.doc.toString();
    const from = before.indexOf('code');
    const to = before.indexOf(' here') + ' here'.length;
    view.dispatch({ selection: EditorSelection.range(from, to) });
    applyFormatting('codeBlock');
    expect(before).toBe('before 中文\ncode ``` here\nafter');
    expect(view.state.doc.toString()).toBe('before 中文\n\n````\ncode ``` here\n````\n\nafter');
    applyFormatting('codeBlock');
    expect(view.state.doc.toString()).toBe('before 中文\n\n````\ncode ``` here\n````\n\nafter');
    expect(undo(view)).toBe(true);
    expect(view.state.doc.toString()).toBe(before);
  });

  test('formats disjoint selections in one undoable action', async () => {
    await renderEditor({}, 'a`b and c`d');
    const view = editorView();
    view.dispatch({ selection: EditorSelection.create([EditorSelection.range(0, 3), EditorSelection.range(8, 11)]) });
    applyFormatting('inlineCode');
    expect(view.state.doc.toString()).toBe('``a`b`` and ``c`d``');
    expect(undo(view)).toBe(true);
    expect(view.state.doc.toString()).toBe('a`b and c`d');
  });

  test('keyboard formatting uses the same undoable editor command', async () => {
    await renderEditor({}, 'hello');
    const view = editorView();
    view.dispatch({ selection: EditorSelection.range(0, 5) });
    const handled = !view.contentDOM.dispatchEvent(new KeyboardEvent('keydown', {
      key: 'b', code: 'KeyB', ctrlKey: true, bubbles: true, cancelable: true
    }));
    expect(handled).toBe(true);
    expect(view.state.doc.toString()).toBe('**hello**');
    expect(undo(view)).toBe(true);
    expect(view.state.doc.toString()).toBe('hello');
  });

  test('keeps CodeMirror at native size with application shortcuts enabled', async () => {
    await renderEditor();
    const surface = target.querySelector<HTMLElement>('.editor-host')!;

    expect(surface.style.transform).toBe('');
    expect(surface.hasAttribute('data-content-scale')).toBe(false);
    expect(surface.dataset.markliteApplicationShortcuts).toBe('true');
    expect(surface.querySelector('.cm-content')).not.toBeNull();
    expect(surface.querySelector('.cm-gutters')).not.toBeNull();
  });

  test('marks dirty immediately, batches content snapshots, and flushes the latest state on destroy', async () => {
    const { onChange, onDirty, onSessionChange } = await renderEditor({ insertSpaces: false });

    let handled = false;
    flushSync(() => {
      handled = pressTab();
    });

    expect(handled).toBe(true);
    flushSync(() => {
      handled = pressTab();
    });
    expect(handled).toBe(true);
    expect(onDirty).toHaveBeenCalledTimes(2);
    expect(onChange).not.toHaveBeenCalled();
    expect(onSessionChange).not.toHaveBeenCalled();

    component!.$destroy();
    component = undefined;

    expect(onChange).toHaveBeenCalledTimes(1);
    expect(onChange.mock.calls[0][1]).toBe('\t\t');
    expect(onChange.mock.calls.at(-1)?.[2]).toBe(1);
    expect(onSessionChange).toHaveBeenCalledTimes(1);
    expect(onSessionChange.mock.calls[0][0]).toBe('tab-1');
    expect(onSessionChange.mock.calls[0][1]).not.toHaveProperty('doc');
  });

  test('does not publish scroll frames when synchronization is disabled but persists once on destroy', async () => {
    const { onScrollSync } = await renderEditor({ syncScroll: false }, 'one\ntwo');
    const scroller = target.querySelector<HTMLElement>('.cm-scroller')!;

    scroller.dispatchEvent(new Event('scroll'));
    await new Promise((resolve) => requestAnimationFrame(() => resolve(undefined)));
    expect(onScrollSync).not.toHaveBeenCalled();

    component!.$destroy();
    component = undefined;
    expect(onScrollSync).toHaveBeenCalledTimes(1);
  });

  test('marks wheel scrolling as user driven and preview alignment as programmatic', async () => {
    const { onScrollSync } = await renderEditor({ syncScroll: true }, 'one\ntwo');
    const scroller = target.querySelector<HTMLElement>('.cm-scroller')!;
    const nextFrame = () => new Promise((resolve) => requestAnimationFrame(() => resolve(undefined)));
    await nextFrame();
    onScrollSync.mockClear();

    scroller.dispatchEvent(new Event('scroll'));
    await nextFrame();
    expect(onScrollSync.mock.lastCall?.[2]).toBe(false);

    scroller.dispatchEvent(new WheelEvent('wheel', { bubbles: true }));
    scroller.dispatchEvent(new Event('scroll'));
    await nextFrame();
    expect(onScrollSync.mock.lastCall?.[2]).toBe(true);

    const view = editorView();
    view.dispatch({ selection: EditorSelection.range(0, 2) });
    const selection = view.state.selection;
    const focused = document.activeElement;
    (component as SvelteComponent & { scrollToLine(line: number): void }).scrollToLine(2);
    expect(view.state.selection.eq(selection)).toBe(true);
    expect(document.activeElement).toBe(focused);
    scroller.dispatchEvent(new Event('scroll'));
    await nextFrame();
    expect(onScrollSync.mock.lastCall?.[2]).toBe(false);
  });

  test('reconfigures the Tab keymap whenever insertSpaces changes at runtime', async () => {
    const { onChange } = await renderEditor({ insertSpaces: true });

    let handled = false;
    flushSync(() => {
      handled = pressTab();
    });
    expect(handled).toBe(false);
    expect(onChange).not.toHaveBeenCalled();

    component!.$set({ settings: editorSettings({ insertSpaces: false }) });
    flushSync();
    onChange.mockClear();
    flushSync(() => {
      handled = pressTab();
    });
    expect(handled).toBe(true);
    expect(onChange).not.toHaveBeenCalled();
    (component as SvelteComponent & { flushContent(): void }).flushContent();
    expect(onChange.mock.calls.some((call) => call[1] !== '')).toBe(true);

    onChange.mockClear();
    component!.$set({ settings: editorSettings({ insertSpaces: true }) });
    flushSync();
    onChange.mockClear();
    flushSync(() => {
      handled = pressTab();
    });
    expect(handled).toBe(false);
    expect(onChange).not.toHaveBeenCalled();
  });

  test('replaces all matches as one undoable edit and clears decorations when search closes', async () => {
    const initialValue = 'a.a A.A a-a';
    const { onChange } = await renderEditor({}, initialValue);
    (component as SvelteComponent & { openReplace(): void }).openReplace();
    await tick();

    const inputs = target.querySelectorAll<HTMLInputElement>('.find-popover input');
    expect(inputs).toHaveLength(2);
    inputs[0].value = 'a.a';
    inputs[0].dispatchEvent(new Event('input', { bubbles: true }));
    inputs[1].value = 'done';
    inputs[1].dispatchEvent(new Event('input', { bubbles: true }));
    await tick();

    target.querySelector<HTMLButtonElement>('button[title="Replace all"]')!.click();
    (component as SvelteComponent & { flushContent(): void }).flushContent();
    expect(onChange.mock.calls.some((call) => call[1] === 'done done a-a')).toBe(true);
    component!.$set({ value: 'done done a-a' });
    flushSync();
    await tick();
    expect(onChange.mock.calls.at(-1)?.[1]).toBe('done done a-a');

    const content = target.querySelector<HTMLElement>('.cm-content')!;
    content.dispatchEvent(
      new KeyboardEvent('keydown', {
        key: 'z',
        code: 'KeyZ',
        ctrlKey: true,
        bubbles: true,
        cancelable: true
      })
    );
    (component as SvelteComponent & { flushContent(): void }).flushContent();
    expect(onChange.mock.calls.some((call) => call[1] === initialValue)).toBe(true);
    component!.$set({ value: initialValue });
    flushSync();
    await tick();
    expect(onChange.mock.calls.at(-1)?.[1]).toBe(initialValue);

    target.querySelector<HTMLButtonElement>('button[title="Close search"]')!.click();
    await tick();
    expect(target.querySelector('.find-popover')).toBeNull();
    expect(target.querySelector('.cm-searchMatch')).toBeNull();

    (component as SvelteComponent & { openFind(): void }).openFind();
    await tick();
    expect(target.querySelector('.find-popover')).not.toBeNull();
    component!.$destroy();
    component = undefined;
    expect(target.innerHTML).toBe('');
  });
});
