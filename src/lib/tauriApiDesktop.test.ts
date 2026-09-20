import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  openUrl: vi.fn(async () => undefined),
  invoke: vi.fn(async () => undefined)
}));

vi.mock('@tauri-apps/plugin-opener', () => ({
  openUrl: mocks.openUrl
}));
vi.mock('@tauri-apps/api/core', () => ({
  invoke: mocks.invoke
}));

import { openEmailLink, openExternalLink } from './tauriApi';

describe('desktop external opener boundary', () => {
  beforeEach(() => {
    (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {};
    mocks.openUrl.mockClear();
    mocks.invoke.mockClear();
  });

  afterEach(() => {
    delete (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
  });

  test('passes only normalized http and https URLs to the Tauri opener', async () => {
    await openExternalLink('https://example.com/a b');
    await openExternalLink('http://example.com/');

    expect(mocks.openUrl).toHaveBeenNthCalledWith(1, 'https://example.com/a%20b');
    expect(mocks.openUrl).toHaveBeenNthCalledWith(2, 'http://example.com/');
  });

  test('rejects dangerous schemes without invoking the Tauri opener', async () => {
    await expect(openExternalLink('javascript:alert(1)')).rejects.toMatchObject({
      code: 'UNSUPPORTED_LINK_SCHEME'
    });
    await expect(openExternalLink('file:///C:/secret.md')).rejects.toMatchObject({
      code: 'UNSUPPORTED_LINK_SCHEME'
    });

    expect(mocks.openUrl).not.toHaveBeenCalled();
  });

  test('opens only a controlled mailto target from a typed address', async () => {
    await openEmailLink('writer@example.org');
    expect(mocks.invoke).toHaveBeenCalledWith('open_validated_email_link', { address: 'writer@example.org' });
    expect(mocks.openUrl).not.toHaveBeenCalled();

    mocks.invoke.mockClear();
    await expect(openEmailLink('writer@example.org?subject=unsafe')).rejects.toMatchObject({
      code: 'INVALID_MARKDOWN_TARGET'
    });
    await expect(openEmailLink('writer@example.org\nBcc:other@example.org')).rejects.toMatchObject({
      code: 'INVALID_MARKDOWN_TARGET'
    });
    expect(mocks.invoke).not.toHaveBeenCalled();
  });
});
