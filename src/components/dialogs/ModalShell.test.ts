import { mount, tick, unmount } from 'svelte';
import { afterEach, describe, expect, test, vi } from 'vitest';
import ModalShellHarness from '../../test/ModalShellHarness.svelte';

let target: HTMLDivElement;
let component: ReturnType<typeof mount> | undefined;

async function render(overrides: Record<string, unknown> = {}) {
  const onClose = vi.fn();
  target = document.createElement('div');
  document.body.append(target);
  component = mount(ModalShellHarness, {
    target,
    props: { onClose, ...overrides }
  });
  await tick();
  return onClose;
}

afterEach(async () => {
  if (component) await unmount(component);
  component = undefined;
  target?.remove();
});

describe('ModalShell focus and close contract', () => {
  test('moves focus inside, traps Tab and restores the opener when unmounted', async () => {
    const opener = document.createElement('button');
    document.body.append(opener);
    opener.focus();
    await render();
    const [first, last] = [...target.querySelectorAll<HTMLButtonElement>('button')];

    await vi.waitFor(() => expect(document.activeElement).toBe(first));
    last.focus();
    last.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', bubbles: true, cancelable: true }));
    expect(document.activeElement).toBe(first);
    first.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', shiftKey: true, bubbles: true, cancelable: true }));
    expect(document.activeElement).toBe(last);

    await unmount(component!);
    component = undefined;
    expect(document.activeElement).toBe(opener);
    opener.remove();
  });

  test('handles Escape and backdrop through one guarded close path', async () => {
    const onClose = await render();
    const dialog = target.querySelector<HTMLElement>('[role="dialog"]')!;
    dialog.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
    target.querySelector<HTMLElement>('.modal-backdrop')!.click();
    expect(onClose).toHaveBeenCalledTimes(2);
  });

  test('locks Escape, backdrop and focus selection while busy', async () => {
    const onClose = await render({ busy: true });
    const dialog = target.querySelector<HTMLElement>('[role="dialog"]')!;
    dialog.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
    target.querySelector<HTMLElement>('.modal-backdrop')!.click();
    expect(onClose).not.toHaveBeenCalled();
    expect(dialog.getAttribute('aria-busy')).toBe('true');
  });
});
