import { expect, test } from 'vitest';
import { createPreferencesWriter } from './preferences';
import type { WorkspacePreferences } from '../../lib/platform/workspace';

test('a slow earlier save finishes before the newest preference write', async () => {
  let release!: () => void;
  const blocked = new Promise<void>(resolve => { release = resolve; });
  const order: string[] = [];
  const writer = createPreferencesWriter(async value => {
    if (value.expanded[0] === 'old') await blocked;
    order.push(value.expanded[0]); return value;
  }, error => { throw error; });
  const root = { kind: 'desktopDirectory', path: '/notes' } as const;
  const value = (name: string): WorkspacePreferences => ({ version: 2, root, expanded: [name], restoreOnStart: true,
    recent: [{ resource: root, name: 'notes' }] });
  void writer.write(value('old')); await Promise.resolve();
  void writer.write(value('intermediate')); void writer.write(value('new'));
  release(); await writer.flush();
  expect(order).toEqual(['old', 'new']);
});
