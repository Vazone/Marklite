import { mount, tick, unmount } from 'svelte';
import { afterEach, expect, test, vi } from 'vitest';
import ExternalChangeDialog from './ExternalChangeDialog.svelte';

let target: HTMLDivElement;
let component: ReturnType<typeof mount> | undefined;

afterEach(async () => {
  if (component) await unmount(component);
  component = undefined;
  target?.remove();
});

test('focuses cancellation and exposes the three explicit conflict resolutions', async () => {
  const callbacks = { onReload: vi.fn(), onSaveCopy: vi.fn(), onOverwrite: vi.fn(), onCancel: vi.fn() };
  target = document.createElement('div');
  document.body.append(target);
  component = mount(ExternalChangeDialog, {
    target,
    props: { open: true, title: 'note.md', path: 'C:\\docs\\note.md', busy: false, ...callbacks }
  });
  await tick();
  const buttons = [...target.querySelectorAll<HTMLButtonElement>('button')];
  const named = (name: string) => buttons.find((button) => button.textContent?.trim() === name)!;
  await vi.waitFor(() => expect(document.activeElement).toBe(named('Cancel')));
  named('Reload from disk').click();
  named('Save a copy').click();
  named('Overwrite explicitly').click();
  expect(callbacks.onReload).toHaveBeenCalledOnce();
  expect(callbacks.onSaveCopy).toHaveBeenCalledOnce();
  expect(callbacks.onOverwrite).toHaveBeenCalledOnce();
});
