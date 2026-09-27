import { expect, test } from 'vitest';
import fixtures from '../../shared/resource-contract-fixtures.json';
import { desktopPath, parseResourceRef, documentResource } from './resources';

test('shared tagged resource contract preserves opaque paths and URIs', () => {
  for (const value of fixtures.valid) expect(parseResourceRef(value)).toEqual(value);
  for (const value of fixtures.invalid) expect(() => parseResourceRef(value)).toThrow();
});

test('document resource preserves legacy paths and rejects conflicting new fields', () => {
  expect(() => documentResource({ path: null, resource: { kind: 'desktopDirectory', path: 'C:\\notes' } })).toThrow();
  expect(documentResource({ path: 'C:\\a.md' })).toEqual({ kind: 'desktopFile', path: 'C:\\a.md' });
  expect(documentResource({ path: null, resource: { kind: 'androidDocument', uri: 'content://provider/document/a' } })).toEqual({ kind: 'androidDocument', uri: 'content://provider/document/a' });
  expect(() => documentResource({ path: 'C:\\a.md', resource: null })).toThrow();
  expect(() => documentResource({ path: 'C:\\a.md', resource: { kind: 'desktopFile', path: 'C:\\b.md' } })).toThrow();
  expect(() => documentResource({ path: 'C:\\a.md', resource: { kind: 'androidDocument', uri: 'content://provider/document/a' } })).toThrow();
});

test('desktop adapter refuses URI references rather than guessing a disk path', () => {
  for (const value of fixtures.valid) {
    const resource = parseResourceRef(value);
    if (resource.kind === 'desktopFile') expect(desktopPath(resource)).toBe(value.path);
    else expect(() => desktopPath(resource)).toThrow(expect.objectContaining({ code: 'RESOURCE_UNSUPPORTED' }));
  }
  for (const uri of ['file:///notes/a.md', 'content://', 'content://provider/a\0b']) {
    expect(() => parseResourceRef({ kind: 'androidDocument', uri })).toThrow();
  }
});
