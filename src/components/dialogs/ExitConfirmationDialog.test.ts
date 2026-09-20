import { mount, tick, unmount } from 'svelte';
import { afterEach, describe, expect, test, vi } from 'vitest';
import type { DirtyExitDocument } from '../../lib/exitProtection';
import ExitConfirmationDialog from './ExitConfirmationDialog.svelte';

let target: HTMLDivElement;
let component: ReturnType<typeof mount> | undefined;

const documents: DirtyExitDocument[] = [
  {
    id: 'saved-path',
    title: 'notes.md',
    path: 'C:\\docs\\notes.md',
    contentRevision: 3
  },
  {
    id: 'untitled',
    title: 'Untitled.md',
    path: null,
    contentRevision: 1
  }
];

async function renderDialog(overrides: Record<string, unknown> = {}) {
  const callbacks = {
    onSave: vi.fn(),
    onDiscard: vi.fn(),
    onCancel: vi.fn()
  };
  target = document.createElement('div');
  document.body.append(target);
  component = mount(ExitConfirmationDialog, {
    target,
    props: {
      open: true,
      documents,
      busy: false,
      busyLabel: '',
      ...callbacks,
      ...overrides
    }
  });
  await tick();
  return callbacks;
}

function button(label: string): HTMLButtonElement {
  return [...target.querySelectorAll<HTMLButtonElement>('button')].find(
    (item) => item.textContent?.trim() === label
  )!;
}

afterEach(async () => {
  if (component) await unmount(component);
  component = undefined;
  target?.remove();
});

describe('ExitConfirmationDialog', () => {
  test('shows dirty documents and exposes three explicit decisions', async () => {
    const callbacks = await renderDialog();

    const dialog = target.querySelector<HTMLElement>('[role="alertdialog"]')!;
    expect(dialog.getAttribute('aria-modal')).toBe('true');
    expect(dialog.textContent).toContain('2 documents contain unsaved content');
    expect(dialog.textContent).toContain('notes.md');
    expect(dialog.textContent).toContain('No save location selected');
    expect(button('Cancel')).toBeTruthy();
    expect(button('Don’t save')).toBeTruthy();
    expect(button('Save and exit')).toBeTruthy();

    button('Save and exit').click();
    button('Don’t save').click();
    button('Cancel').click();
    expect(callbacks.onSave).toHaveBeenCalledOnce();
    expect(callbacks.onDiscard).toHaveBeenCalledOnce();
    expect(callbacks.onCancel).toHaveBeenCalledOnce();
  });

  test('focuses the safe cancel action and Escape cancels', async () => {
    const callbacks = await renderDialog();

    await vi.waitFor(() => expect(document.activeElement).toBe(button('Cancel')));
    target
      .querySelector<HTMLElement>('[role="alertdialog"]')!
      .dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));

    expect(callbacks.onCancel).toHaveBeenCalledOnce();
  });

  test('locks every decision while saving and announces progress', async () => {
    const callbacks = await renderDialog({ busy: true, busyLabel: '正在保存未保存的文档…' });

    const dialog = target.querySelector<HTMLElement>('[role="alertdialog"]')!;
    expect(dialog.getAttribute('aria-busy')).toBe('true');
    expect(dialog.textContent).toContain('正在保存未保存的文档…');
    expect([...dialog.querySelectorAll<HTMLButtonElement>('button')].every((item) => item.disabled)).toBe(true);

    dialog.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    expect(callbacks.onCancel).not.toHaveBeenCalled();
  });
});
