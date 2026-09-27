import { expect, test } from 'vitest';
import { mergeMetadata } from './metadata';
import type { WorkspaceEntry } from '../../lib/platform/workspace';
const entry: WorkspaceEntry = { name: 'a.md', relativePath: 'a.md', kind: 'markdown', resource: null, size: null, issue: null };

test('global metadata entry limit spans folders and releasing a folder permits subsequent pages', () => {
  const nodes = new Map([['a', { entries: Array(100000).fill(entry) }], ['b', { entries: Array(100000).fill(entry) }]]);
  expect(() => mergeMetadata(nodes, 'b', [entry], true)).toThrowError(/metadata limit/);
  expect(nodes.get('b')?.entries).toHaveLength(100000);
  nodes.delete('a');
  expect(mergeMetadata(nodes, 'b', [entry], true)).toHaveLength(100001);
});

test('long names reach the byte budget before the entry limit and replacing a page releases prior accounting', () => {
  const long = { ...entry, name: 'x'.repeat(32768) };
  const nodes = new Map([['', { entries: Array(1000).fill(long) }]]);
  expect(() => mergeMetadata(nodes, '', Array(30).fill(long), true)).toThrowError(/metadata limit/);
  expect(mergeMetadata(nodes, '', [entry], false)).toEqual([entry]);
});
