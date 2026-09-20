import { mount, tick, unmount } from 'svelte';
import { afterEach, describe, expect, test, vi } from 'vitest';
import ExportDialog from './ExportDialog.svelte';

let target: HTMLDivElement;
let component: ReturnType<typeof mount> | undefined;

async function renderDialog(overrides: Record<string, unknown> = {}) {
  const callbacks = { onExport: vi.fn(), onClose: vi.fn() };
  target = document.createElement('div');
  document.body.append(target);
  component = mount(ExportDialog, {
    target,
    props: {
      open: true,
      busy: false,
      documentTitle: 'notes.md',
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

describe('ExportDialog', () => {
  test('offers HTML, PDF, DOCX and the single SVG mind-map format through one modal', async () => {
    const callbacks = await renderDialog();
    const dialog = target.querySelector<HTMLElement>('[role="dialog"]')!;
    expect(dialog.getAttribute('aria-modal')).toBe('true');
    expect(dialog.textContent).toContain('notes.md');
    expect(
      [...dialog.querySelectorAll<HTMLSelectElement>('select')[0].options].map((option) => option.value)
    ).toEqual(['html', 'pdf', 'docx', 'png', 'svg']);

    dialog.querySelector<HTMLFormElement>('form')!.dispatchEvent(
      new SubmitEvent('submit', { bubbles: true, cancelable: true })
    );
    expect(callbacks.onExport).toHaveBeenCalledWith(
      'html',
      expect.objectContaining({
        paperSize: 'a4',
        orientation: 'portrait',
        includeTitle: true,
        includeLocalImages: false
      })
    );
  });

  test('explains complete SVG mind-map export and hides page-only options', async () => {
    const callbacks = await renderDialog();
    const format = target.querySelector<HTMLSelectElement>('select')!;
    format.value = 'svg';
    format.dispatchEvent(new Event('change', { bubbles: true }));
    await tick();

    expect(target.textContent).toContain('fully expanded heading mind map');
    expect(target.textContent).not.toContain('Paper');
    expect(target.textContent).not.toContain('Embed local images');
    target.querySelector<HTMLFormElement>('form')!.dispatchEvent(
      new SubmitEvent('submit', { bubbles: true, cancelable: true })
    );
    expect(callbacks.onExport).toHaveBeenCalledWith('svg', expect.any(Object));
  });

  test('Escape cancels when idle and busy state locks every decision', async () => {
    const idle = await renderDialog();
    target.querySelector<HTMLElement>('[role="dialog"]')!.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })
    );
    expect(idle.onClose).toHaveBeenCalledOnce();
    await unmount(component!);
    component = undefined;
    target.remove();

    const busy = await renderDialog({ busy: true });
    const dialog = target.querySelector<HTMLElement>('[role="dialog"]')!;
    expect(dialog.getAttribute('aria-busy')).toBe('true');
    expect([...dialog.querySelectorAll<HTMLInputElement | HTMLButtonElement | HTMLSelectElement>('input, button, select')].every((item) => item.disabled)).toBe(true);
    dialog.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    expect(busy.onClose).not.toHaveBeenCalled();
    expect(button('Exporting…').disabled).toBe(true);
  });
});

test('PNG describes chapter directory output and hides paper options', async () => {
  const callbacks = await renderDialog();
  const format = target.querySelector<HTMLSelectElement>('select')!;
  format.value = 'png';
  format.dispatchEvent(new Event('change', { bubbles: true }));
  await tick();
  expect(target.textContent).toContain('One image per chapter');
  expect(target.textContent).not.toContain('Paper');
  expect(button('Export images').disabled).toBe(false);
  button('Export images').click();
  expect(callbacks.onExport).toHaveBeenCalledWith('png', expect.any(Object));
});
