import { expect, test, vi } from 'vitest';
import { createNativeController } from './nativeController';

test('late native registration is released and queued paths cannot open after dispose', async () => {
  let ready!: (unlisten: () => void) => void;
  let receive!: () => void;
  const unlisten = vi.fn();
  const drain = vi.fn(async () => ['late.md']);
  const open = vi.fn();
  const controller = createNativeController({
    enabled: true,
    events: { onDrop: vi.fn(), onClose: vi.fn(), onOpen: callback => {
      receive = callback;
      return new Promise(resolve => { ready = resolve; });
    } },
    drain, open, close: vi.fn(), onError: vi.fn(), onCloseUnavailable: vi.fn()
  });
  const pending = controller.externalOpen();
  controller.dispose();
  ready(unlisten);
  await pending;
  receive();
  controller.dispose();
  expect(unlisten).toHaveBeenCalledTimes(1);
  expect(drain).not.toHaveBeenCalled();
  expect(open).not.toHaveBeenCalled();
});

test('each mounted owner releases all native subscriptions exactly once', async () => {
  const releases = [vi.fn(), vi.fn(), vi.fn(), vi.fn(), vi.fn(), vi.fn()];
  let index = 0;
  const register = vi.fn(async () => releases[index++]);
  for (let mount = 0; mount < 2; mount++) {
    const owner = createNativeController({ enabled: true,
      events: { onDrop: register, onOpen: register, onClose: register },
      drain: async () => [], open: vi.fn(), close: vi.fn(), onError: vi.fn(), onCloseUnavailable: vi.fn() });
    await owner.dragDrop(); await owner.externalOpen(); await owner.closeProtection();
    owner.dispose(); owner.dispose();
  }
  expect(register).toHaveBeenCalledTimes(6);
  for (const release of releases) expect(release).toHaveBeenCalledTimes(1);
});
