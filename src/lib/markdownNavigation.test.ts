import { describe, expect, test, vi } from 'vitest';
import { executeMarkdownTarget, type MarkdownNavigationHandlers } from './markdownNavigation';

function handlers(): MarkdownNavigationHandlers {
  return {
    scrollToFragment: vi.fn(),
    openDocument: vi.fn(async () => true),
    confirmExternal: vi.fn(() => true),
    openExternal: vi.fn(async () => undefined),
    openEmail: vi.fn(async () => undefined)
  };
}

describe('typed Markdown navigation dispatch', () => {
  test('opens local documents in MarkLite and never sends them to the external opener', async () => {
    const actions = handlers();

    await executeMarkdownTarget(
      { kind: 'localDocument', path: 'C:\\docs\\other.md', fragment: 'part' },
      true,
      actions
    );

    expect(actions.openDocument).toHaveBeenCalledWith('C:\\docs\\other.md', 'part');
    expect(actions.openExternal).not.toHaveBeenCalled();
    expect(actions.confirmExternal).not.toHaveBeenCalled();
  });

  test('waits for local document navigation and propagates cancellation', async () => {
    const actions = handlers();
    let finish!: (opened: boolean) => void;
    vi.mocked(actions.openDocument).mockImplementation(
      () => new Promise<boolean>((resolve) => (finish = resolve))
    );

    const result = executeMarkdownTarget(
      { kind: 'localDocument', path: 'C:\\docs\\other.md', fragment: null },
      true,
      actions
    );
    let settled = false;
    void result.finally(() => (settled = true));
    await Promise.resolve();
    expect(settled).toBe(false);

    finish(false);
    await expect(result).resolves.toBe(false);
  });

  test('uses confirmation only for external links', async () => {
    const actions = handlers();
    vi.mocked(actions.confirmExternal).mockReturnValue(false);

    await expect(
      executeMarkdownTarget({ kind: 'external', url: 'https://example.com/' }, true, actions)
    ).resolves.toBe(false);

    expect(actions.confirmExternal).toHaveBeenCalledWith('https://example.com/');
    expect(actions.openExternal).not.toHaveBeenCalled();
  });

  test('opens an external link directly when confirmation is disabled', async () => {
    const actions = handlers();

    await expect(
      executeMarkdownTarget({ kind: 'external', url: 'https://example.com/' }, false, actions)
    ).resolves.toBe(true);

    expect(actions.confirmExternal).not.toHaveBeenCalled();
    expect(actions.openExternal).toHaveBeenCalledWith('https://example.com/');
  });

  test('confirms and dispatches a typed email without using the web opener', async () => {
    const actions = handlers();

    await expect(
      executeMarkdownTarget({ kind: 'email', address: 'writer@example.org' }, true, actions)
    ).resolves.toBe(true);

    expect(actions.confirmExternal).toHaveBeenCalledWith('mailto:writer@example.org');
    expect(actions.openEmail).toHaveBeenCalledWith('writer@example.org');
    expect(actions.openExternal).not.toHaveBeenCalled();
  });

  test('handles anchors without invoking document or external navigation', async () => {
    const actions = handlers();

    await executeMarkdownTarget({ kind: 'anchor', fragment: '标题' }, true, actions);

    expect(actions.scrollToFragment).toHaveBeenCalledWith('标题');
    expect(actions.openDocument).not.toHaveBeenCalled();
    expect(actions.openExternal).not.toHaveBeenCalled();
  });
});
