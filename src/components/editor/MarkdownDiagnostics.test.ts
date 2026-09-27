import { mount, tick, unmount } from 'svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import MarkdownDiagnostics from './MarkdownDiagnostics.svelte';
import { setLanguage } from '../../lib/i18n';

afterEach(() => setLanguage('en'));

describe('Markdown diagnostics', () => {
  it('shows a localized location and navigates without interpreting message HTML', async () => {
    setLanguage('zh-CN');
    const target = document.createElement('div');
    const onJumpToLine = vi.fn();
    const component = mount(MarkdownDiagnostics, { target, props: {
      diagnostics: [{ code: 'FRONT_MATTER_INVALID', message: '<img src=x onerror=alert(1)>', line: 4, column: 7 }],
      onJumpToLine
    } });
    await tick();
    expect(target.textContent).toContain('第 4 行，第 7 列');
    expect(target.textContent).toContain('YAML 格式无效');
    expect(target.querySelector('img')).toBeNull();
    target.querySelector('button')!.click();
    expect(onJumpToLine).toHaveBeenCalledWith(4);
    setLanguage('en');
    await tick();
    expect(target.textContent).toContain('Line 4, column 7');
    expect(target.textContent).toContain('Invalid YAML');
    await unmount(component);
  });

  it('renders no panel for a valid document', async () => {
    const target = document.createElement('div');
    const component = mount(MarkdownDiagnostics, { target, props: { diagnostics: [], onJumpToLine: vi.fn() } });
    await tick();
    expect(target.querySelector('aside')).toBeNull();
    await unmount(component);
  });
});
