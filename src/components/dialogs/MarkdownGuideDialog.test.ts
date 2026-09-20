import { mount, tick, unmount } from 'svelte';
import { afterEach, describe, expect, test, vi } from 'vitest';
import MarkdownGuideDialog from './MarkdownGuideDialog.svelte';

let target: HTMLDivElement;
let component: ReturnType<typeof mount> | undefined;

async function render() {
  target = document.createElement('div');
  document.body.append(target);
  component = mount(MarkdownGuideDialog, {
    target,
    props: { open: true, onClose: vi.fn() }
  });
  await tick();
}

afterEach(async () => {
  if (component) await unmount(component);
  component = undefined;
  target?.remove();
});

describe('MarkdownGuideDialog', () => {
  test('navigates original offline groups and filters the selected group', async () => {
    await render();
    expect(target.textContent).toContain('ATX headings');
    expect(target.textContent).toContain('Images');

    [...target.querySelectorAll<HTMLButtonElement>('.markdown-guide-nav > button')]
      .find((button) => button.textContent?.includes('Extended syntax'))!
      .click();
    await tick();
    expect(target.textContent).toContain('Tables');
    expect(target.textContent).toContain('Not supported');

    const input = target.querySelector<HTMLInputElement>('[aria-label="Search Markdown syntax"]')!;
    input.value = '- [x]';
    input.dispatchEvent(new InputEvent('input', { bubbles: true }));
    await tick();
    const topics = target.querySelector('.markdown-guide-topics')!;
    expect(topics.querySelectorAll('.markdown-guide-topic')).toHaveLength(1);
    expect(topics.textContent).toContain('Task lists');
    expect(topics.textContent).not.toContain('Footnotes');
  });

  test('contains no external learning links or reference actions', async () => {
    await render();
    expect(target.querySelector('a')).toBeNull();
    expect(target.textContent).not.toContain('Further reading');
    expect(target.querySelector('.markdown-guide-references')).toBeNull();
  });

  test('copies only the selected original example', async () => {
    const descriptor = Object.getOwnPropertyDescriptor(navigator, 'clipboard');
    const writeText = vi.fn(async (_text: string) => undefined);
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { writeText }
    });
    try {
      await render();
      target.querySelector<HTMLButtonElement>('.markdown-guide-topic button')!.click();
      await vi.waitFor(() => expect(writeText).toHaveBeenCalledOnce());
      expect(writeText.mock.calls[0][0]).toContain('# Heading 1');
      await vi.waitFor(() => expect(target.textContent).toContain('Copied'));
    } finally {
      if (descriptor) Object.defineProperty(navigator, 'clipboard', descriptor);
      else Reflect.deleteProperty(navigator, 'clipboard');
    }
  });
});
