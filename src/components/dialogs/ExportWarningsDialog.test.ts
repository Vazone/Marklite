import { mount, tick, unmount } from 'svelte';
import { afterEach, describe, expect, test, vi } from 'vitest';
import ExportWarningsDialog from './ExportWarningsDialog.svelte';
import type { ExportWarningReport } from '../../lib/exportWarningReport';

const report: ExportWarningReport = {
  jobId: 'job-7', format: 'docx', outputPath: 'C:/out.docx', documentTitle: 'Original',
  sourcePath: 'C:/source.md', tabId: 'tab-1', contentRevision: 4,
  warnings: [{ code: 'DIAGRAM_PARSE_FAILED', message: 'bad syntax', target: 'diagram-0:12-20', line: 3, excerpt: 'A[-->' }]
};
let target: HTMLDivElement;
let instance: ReturnType<typeof mount> | undefined;

afterEach(async () => {
  if (instance) await unmount(instance);
  target?.remove();
  vi.restoreAllMocks();
});

describe('ExportWarningsDialog', () => {
  test('shows source identity, warning target and location; copies the exact job report', async () => {
    target = document.createElement('div');
    document.body.append(target);
    const copy = vi.fn(async (_text: string) => {});
    const old = Object.getOwnPropertyDescriptor(navigator, 'clipboard');
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: copy } });
    const jump = vi.fn();
    try {
      instance = mount(ExportWarningsDialog, { target, props: { report, canJump: true, onJumpToLine: jump, onClose: vi.fn() } });
      await tick();
      expect(target.textContent).toContain('job-7');
      expect(target.textContent).toContain('source.md');
      expect(target.textContent).toContain('DIAGRAM_PARSE_FAILED');
      target.querySelector<HTMLButtonElement>('li button')!.click();
      expect(jump).toHaveBeenCalledWith(3);
      [...target.querySelectorAll<HTMLButtonElement>('button')].find((button) => button.textContent?.includes('Copy report'))!.click();
      await vi.waitFor(() => expect(copy).toHaveBeenCalledOnce());
      expect(copy.mock.calls[0][0]).toContain('revision 4');
    } finally {
      if (old) Object.defineProperty(navigator, 'clipboard', old);
      else Reflect.deleteProperty(navigator, 'clipboard');
    }
  });
});
