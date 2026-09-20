import { mount, tick, unmount } from 'svelte';
import { afterEach, describe, expect, test, vi } from 'vitest';
import AboutDialog from './AboutDialog.svelte';

let target: HTMLDivElement;
let component: ReturnType<typeof mount> | undefined;

async function renderDialog() {
  const callbacks = {
    onClose: vi.fn(),
    onExportDiagnostics: vi.fn(),
    onClearDiagnostics: vi.fn(),
    onOpenRepository: vi.fn()
  };
  target = document.createElement('div');
  document.body.append(target);
  component = mount(AboutDialog, { target, props: { open: true, ...callbacks } });
  await tick();
  return callbacks;
}

afterEach(async () => {
  if (component) await unmount(component);
  component = undefined;
  target?.remove();
});

describe('AboutDialog managed actions', () => {
  test('routes the repository URL through the managed callback', async () => {
    const callbacks = await renderDialog();
    const repository = [...target.querySelectorAll<HTMLButtonElement>('button')].find((button) =>
      button.textContent?.includes('https://github.com/Vazone/Marklite')
    )!;

    repository.click();

    expect(callbacks.onOpenRepository).toHaveBeenCalledOnce();
    expect(callbacks.onOpenRepository).toHaveBeenCalledWith('https://github.com/Vazone/Marklite');
  });

  test('receives focus and closes through the shared Escape path', async () => {
    const callbacks = await renderDialog();
    const dialog = target.querySelector<HTMLElement>('[role="dialog"]')!;
    await vi.waitFor(() => expect(dialog.contains(document.activeElement)).toBe(true));

    dialog.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));

    expect(callbacks.onClose).toHaveBeenCalledOnce();
  });
});
