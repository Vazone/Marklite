import { describe, expect, test } from 'vitest';
import { isSameFileIdentity, isSameFilePath } from './filePathIdentity';

describe('opaque filesystem identity', () => {
  test('compares backend identities without parsing or normalization', () => {
    const identity = 'marklite-file:v1:windows:12345678:1234567890abcdef';
    expect(isSameFileIdentity(identity, identity)).toBe(true);
    expect(isSameFileIdentity(identity, identity.toUpperCase())).toBe(false);
    expect(isSameFileIdentity(` ${identity}`, identity)).toBe(false);
  });

  test('does not treat absent identities as owners', () => {
    expect(isSameFileIdentity(null, null)).toBe(false);
    expect(isSameFileIdentity('', '')).toBe(false);
    expect(isSameFileIdentity(undefined, 'identity')).toBe(false);
  });

  test('keeps unloaded session path correlation exact and identity-neutral', () => {
    expect(isSameFilePath('/Users/name/note.md ', '/Users/name/note.md ')).toBe(true);
    expect(isSameFilePath('/Users/name/Note.md', '/Users/name/note.md')).toBe(false);
    expect(isSameFilePath('C:\\Docs\\note.md', 'c:\\docs\\note.md')).toBe(false);
    expect(isSameFilePath('/docs/./note.md', '/docs/note.md')).toBe(false);
  });
});
