import { createClassComponent } from 'svelte/legacy';
import { tick, type SvelteComponent } from 'svelte';
import { afterEach, describe, expect, test } from 'vitest';
import StatusBar from './StatusBar.svelte';
import { setLanguage } from '../../lib/i18n';

let target: HTMLDivElement;
let component: (SvelteComponent & { $set(props: Record<string, unknown>): void; $destroy(): void }) | undefined;

afterEach(() => {
  setLanguage('en');
  component?.$destroy();
  component = undefined;
  target?.remove();
});

describe('StatusBar', () => {
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
