import { mount, tick, unmount } from 'svelte';
import { afterEach, describe, expect, test, vi } from 'vitest';
import type { CommandItem } from '../../lib/commands';
import CommandPalette from './CommandPalette.svelte';

let target: HTMLDivElement;
let component: ReturnType<typeof mount> | undefined;

async function render(commands: CommandItem[]) {
  const onClose = vi.fn();
  target = document.createElement('div');
  document.body.append(target);
  component = mount(CommandPalette, {
    target,
    props: { open: true, commands, onClose }
  });
  await tick();
  return onClose;
}

afterEach(async () => {
  if (component) await unmount(component);
  component = undefined;
  target?.remove();
});

describe('CommandPalette behavior', () => {
  test('recovers selection after an empty result and runs the first restored command', async () => {
    const action = vi.fn();
    const onClose = await render([
      { id: 'new', title: '新建文件', category: '文件', action },
      { id: 'open', title: '打开文件', category: '文件', action: vi.fn() }
    ]);
    const input = target.querySelector<HTMLInputElement>('input')!;
    await vi.waitFor(() => expect(document.activeElement).toBe(input));

    input.value = '不存在';
    input.dispatchEvent(new InputEvent('input', { bubbles: true }));
    await tick();
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }));

    input.value = '';
    input.dispatchEvent(new InputEvent('input', { bubbles: true }));
    await tick();
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));

    expect(action).toHaveBeenCalledOnce();
    expect(onClose).toHaveBeenCalledOnce();
  });

  test('uses the shared modal Escape path without a window-level listener', async () => {
    const onClose = await render([]);
    const dialog = target.querySelector<HTMLElement>('[role="dialog"]')!;
    dialog.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    expect(onClose).toHaveBeenCalledOnce();
  });
});
