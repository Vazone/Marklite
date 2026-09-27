import { expect, test, vi } from 'vitest';
import fixture from '../../shared/recovery-contract-fixtures.json';
import { parseRecoveryInventory, parseRecoveryReceipt, parseRecoverySnapshot } from './recovery';
import { createTauriClient } from './tauriClient';

test('recovery IPC round-trips the shared source and durable receipt contracts', async () => {
  const snapshot = parseRecoverySnapshot(fixture.snapshot);
  const receipt = parseRecoveryReceipt(fixture.receipt);
  expect(snapshot).toEqual(fixture.snapshot);
  expect(parseRecoveryInventory(fixture.inventory)).toEqual(fixture.inventory);
  const invoke = vi.fn(async (command: string) => {
    if (command === 'read_recovery') return fixture.snapshot;
    if (command === 'save_recovery') return fixture.receipt;
    if (command === 'list_recovery') return fixture.inventory;
    return undefined;
  });
  const client = createTauriClient({ invoke });
  await expect(client.listRecovery()).resolves.toEqual(fixture.inventory);
  await expect(client.readRecovery(snapshot.id)).resolves.toEqual(snapshot);
  await expect(client.saveRecovery(snapshot)).resolves.toEqual(receipt);
  await client.resolveRecovery(receipt);
  expect(invoke.mock.calls).toEqual([
    ['list_recovery', undefined], ['read_recovery', { id: snapshot.id }],
    ['save_recovery', { snapshot }], ['resolve_recovery', { receipt }]
  ]);
});

test('invalid recovery acknowledgements and malformed references are rejected', () => {
  expect(() => parseRecoverySnapshot({ ...fixture.snapshot, resource: { kind: 'desktopDirectory', path: '/notes' } })).toThrow();
  expect(() => parseRecoveryReceipt({ ...fixture.receipt, revision: Number.MAX_SAFE_INTEGER + 1 })).toThrow();
  expect(() => parseRecoveryReceipt({ ...fixture.receipt, checksum: 'ok' })).toThrow();
  expect(() => parseRecoverySnapshot({ ...fixture.snapshot, resource: { kind: 'file', path: '/guess' } })).toThrow();
});
