import { expect, test, vi } from 'vitest';
import { createRecoveryCoordinator } from './recoveryCoordinator';
import type { RecoveryReceipt, RecoverySnapshot } from './platform/recovery';

const snapshot = (revision: number): RecoverySnapshot => ({ id: 'doc-1', revision, title: 'Untitled', content: `${revision}`, resource: null, baseFileIdentity: null, baseContentVersion: null });
const receipt = (revision: number): RecoveryReceipt => ({ id: 'doc-1', revision, checksum: `sha256:${'0'.repeat(64)}`, updatedAt: '2026-09-23T00:00:00Z' });

test('flush has a fixed revision boundary while later edits remain queued', async () => {
  let release!: () => void;
  const gate = new Promise<void>(resolve => { release = resolve; });
  const save = vi.fn(async (s: RecoverySnapshot) => { if (s.revision === 1) await gate; return receipt(s.revision); });
  const coordinator = createRecoveryCoordinator({ save, resolve: vi.fn(), onError: vi.fn() });
  coordinator.queue(snapshot(1));
  const flushing = coordinator.flush();
  coordinator.queue(snapshot(2)); coordinator.queue(snapshot(3));
  release(); await flushing;
  expect(save.mock.calls.map(([s]) => s.revision)).toEqual([1]);
  expect(coordinator.receipt('doc-1')?.revision).toBe(1);
  await coordinator.flush();
  expect(save.mock.calls.map(([s]) => s.revision)).toEqual([1, 3]);
  expect(coordinator.receipt('doc-1')?.revision).toBe(3);
  coordinator.dispose();
});

test('retirement waits for the write acknowledgement and uses its exact receipt', async () => {
  let release!: () => void;
  const gate = new Promise<void>(resolve => { release = resolve; });
  const resolve = vi.fn();
  const coordinator = createRecoveryCoordinator({ save: async () => { await gate; return receipt(1); }, resolve, onError: vi.fn() });
  coordinator.queue(snapshot(1)); const flushing = coordinator.flush();
  coordinator.retire('doc-1');
  const retired = coordinator.flush();
  expect(resolve).not.toHaveBeenCalled(); release(); await flushing; await retired;
  expect(resolve).toHaveBeenCalledWith(receipt(1));
  expect(coordinator.receipt('doc-1')).toBeUndefined();
  coordinator.dispose();
});

test('failed flush retains pending content for retry and never acknowledges it', async () => {
  const save = vi.fn().mockRejectedValueOnce(new Error('disk full')).mockResolvedValue(receipt(2));
  const coordinator = createRecoveryCoordinator({ save, resolve: vi.fn(), onError: vi.fn() });
  coordinator.queue(snapshot(2)); await expect(coordinator.flush()).rejects.toThrow('disk full');
  expect(coordinator.receipt('doc-1')).toBeUndefined();
  await coordinator.flush(); expect(coordinator.receipt('doc-1')?.revision).toBe(2);
  coordinator.dispose();
});

test('mismatched acknowledgement is rejected without retiring the copy', async () => {
  const resolve = vi.fn();
  const coordinator = createRecoveryCoordinator({ save: async () => receipt(0), resolve, onError: vi.fn() });
  coordinator.queue(snapshot(1));
  await expect(coordinator.flush()).rejects.toMatchObject({ code: 'INVALID_RECOVERY_RECEIPT' });
  expect(resolve).not.toHaveBeenCalled(); coordinator.dispose();
});

test('continuous edits do not postpone the first scheduled persistence indefinitely', async () => {
  vi.useFakeTimers();
  const save = vi.fn(async (s: RecoverySnapshot) => receipt(s.revision));
  const coordinator = createRecoveryCoordinator({ save, resolve: vi.fn(), onError: vi.fn(), delayMs: 100 });
  coordinator.queue(snapshot(1)); await vi.advanceTimersByTimeAsync(60); coordinator.queue(snapshot(2));
  await vi.advanceTimersByTimeAsync(40); expect(save).toHaveBeenCalledWith(snapshot(2));
  coordinator.dispose(); vi.useRealTimers();
});
