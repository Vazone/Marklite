import { createClassComponent } from 'svelte/legacy';
import { tick, type SvelteComponent } from 'svelte';
import { afterEach, describe, expect, test } from 'vitest';
import StatusBar from './StatusBar.svelte';
import { setLanguage } from '../../lib/i18n';
import type { StatusDocumentView } from '../../app/stores/documentStore';

let target: HTMLDivElement;
let component: (SvelteComponent & { $set(props: Record<string, unknown>): void; $destroy(): void }) | undefined;

afterEach(() => {
  setLanguage('en');
  component?.$destroy();
  component = undefined;
  target?.remove();
});

describe('StatusBar', () => {
  test('keeps mobile status focused on save state, title and layout', () => {
    target = document.createElement('div');
    document.body.append(target);
    const tab = {
      title: 'Index.md', path: 'content://documents/Index.md', isDirty: true, loadState: 'loaded',
      stats: { wordCount: 120, characterCount: 700, lineCount: 12, headingCount: 2, linkCount: 0, imageCount: 0 },
      cursorPosition: { line: 3, column: 4 }
    } as StatusDocumentView;
    component = createClassComponent({
      component: StatusBar,
      target,
      props: { layoutMode: 'split', tab, mobile: true }
    }) as typeof component;

    expect([...target.querySelectorAll('.statusbar > span')].map(span => span.textContent?.trim()))
      .toEqual(['Unsaved', 'Index.md', 'Split']);
    expect(target.querySelector('.status-document')?.getAttribute('title')).toBe(tab.path);
  });

  test('updates the layout mode without a content zoom indicator', async () => {
    target = document.createElement('div');
    document.body.append(target);
    component = createClassComponent({
      component: StatusBar,
      target,
      props: { layoutMode: 'split', tab: undefined }
    }) as typeof component;

    expect(target.querySelector('.zoom-level')).toBeNull();
    expect(target.querySelector('.mode')?.textContent).toBe('Split');
    component!.$set({ layoutMode: 'edit' });
    await tick();
    expect(target.querySelector('.mode')?.textContent).toBe('Edit');
  });

  test('updates an already-mounted view when the language changes', async () => {
    target = document.createElement('div');
    document.body.append(target);
    component = createClassComponent({
      component: StatusBar,
      target,
      props: { layoutMode: 'split', tab: undefined }
    }) as typeof component;

    expect(target.querySelector('.mode')?.textContent).toBe('Split');
    setLanguage('zh-CN');
    await tick();
    expect(target.querySelector('.mode')?.textContent).toBe('分栏');
  });
});
