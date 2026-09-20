import { mount, tick, unmount } from 'svelte';
import { afterEach, describe, expect, test, vi } from 'vitest';
import TitleBar from './TitleBar.svelte';

let target: HTMLDivElement;
let component: ReturnType<typeof mount> | undefined;

async function renderTitleBar(
  sidebarVisible: boolean,
  onToggleSidebar = vi.fn(),
  onMarkdownGuide = vi.fn()
) {
  target = document.createElement('div');
  document.body.append(target);
  component = mount(TitleBar, {
    target,
    props: {
      sidebarVisible,
      onToggleSidebar,
      onNew: vi.fn(),
      onOpen: vi.fn(),
      onSave: vi.fn(),
      onSaveAs: vi.fn(),
      onExport: vi.fn(),
      onFind: vi.fn(),
      onSettings: vi.fn(),
      onLayout: vi.fn(),
      onCommandPalette: vi.fn(),
      onMarkdownGuide,
      onAbout: vi.fn()
    }
  });
  await tick();
  return { onToggleSidebar, onMarkdownGuide };
}

afterEach(async () => {
  if (component) await unmount(component);
  component = undefined;
  target?.remove();
});

describe('TitleBar sidebar recovery', () => {
  test('shows a labeled recovery action while the sidebar is hidden', async () => {
    const { onToggleSidebar } = await renderTitleBar(false);
    const restore = target.querySelector<HTMLButtonElement>('[aria-label="Expand sidebar"]')!;

    expect(restore).toBeTruthy();
    expect(restore.classList.contains('restore')).toBe(true);
    expect(restore.textContent?.trim()).toBe('Expand sidebar');
    restore.click();
    expect(onToggleSidebar).toHaveBeenCalledOnce();
  });

  test('keeps the expanded-state action compact and explicit to assistive technology', async () => {
    await renderTitleBar(true);
    const collapse = target.querySelector<HTMLButtonElement>('[aria-label="Collapse sidebar"]')!;

    expect(collapse).toBeTruthy();
    expect(collapse.classList.contains('restore')).toBe(false);
    expect(collapse.textContent?.trim()).toBe('');
  });

  test('provides a direct Markdown learning entry', async () => {
    const onMarkdownGuide = vi.fn();
    await renderTitleBar(true, vi.fn(), onMarkdownGuide);
    target.querySelector<HTMLButtonElement>('[aria-label="Markdown syntax guide"]')!.click();
    expect(onMarkdownGuide).toHaveBeenCalledOnce();
  });
});
