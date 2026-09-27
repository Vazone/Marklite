import { mount, tick, unmount } from 'svelte';
import { afterEach, expect, test, vi } from 'vitest';
import { api, pickDiagramPackPath } from '../../lib/tauriApi';
import DiagramRuntimeControl from './DiagramRuntimeControl.svelte';

vi.mock('../../lib/tauriApi', () => ({
  api: {
    getDiagramRuntimeStatus: vi.fn(),
    installDiagramRuntime: vi.fn(),
    uninstallDiagramRuntime: vi.fn()
  },
  pickDiagramPackPath: vi.fn(),
  toAppError: (error: unknown) => error
}));

let target: HTMLDivElement | undefined;
let component: ReturnType<typeof mount> | undefined;

afterEach(async () => {
  if (component) await unmount(component);
  target?.remove();
  component = undefined;
  target = undefined;
  vi.clearAllMocks();
});

test('installs and uninstalls a local Mermaid pack through the native commands', async () => {
  vi.mocked(api.getDiagramRuntimeStatus).mockResolvedValue({ rendererId: 'mermaid-offline-11.17.2', installed: false });
  vi.mocked(pickDiagramPackPath).mockResolvedValue('/tmp/mermaid-pack.zip');
  vi.mocked(api.installDiagramRuntime).mockResolvedValue({ rendererId: 'mermaid-offline-11.17.2', installed: true });
  vi.mocked(api.uninstallDiagramRuntime).mockResolvedValue({ rendererId: 'mermaid-offline-11.17.2', installed: false });
  const onChanged = vi.fn();
  target = document.createElement('div');
  document.body.append(target);
  component = mount(DiagramRuntimeControl, { target, props: { onChanged } });

  await vi.waitFor(() => expect(target?.textContent).toContain('Not installed'));
  target.querySelector<HTMLButtonElement>('.diagram-runtime-actions button')!.click();
  await vi.waitFor(() => expect(target?.textContent).toContain('Installed.'));
  expect(api.installDiagramRuntime).toHaveBeenCalledWith('/tmp/mermaid-pack.zip');
  expect(onChanged).toHaveBeenCalledTimes(1);

  await tick();
  target.querySelectorAll<HTMLButtonElement>('.diagram-runtime-actions button')[1].click();
  await vi.waitFor(() => expect(target?.textContent).toContain('Not installed'));
  expect(api.uninstallDiagramRuntime).toHaveBeenCalledTimes(1);
  expect(onChanged).toHaveBeenCalledTimes(2);
});
