import { expect, test } from 'vitest';
import { browserCapabilities, parseCapabilities, requireExport } from './capabilities';
import { invokeBrowserCommand } from './browserAdapter';
import { createTauriClient } from './tauriClient';

test('browser advertises no native exports and rejects requests explicitly', async () => {
  const client = createTauriClient({ invoke: invokeBrowserCommand });
  const capabilities = await client.getPlatformCapabilities();
  expect(capabilities).toEqual(browserCapabilities);
  for (const format of ['html', 'pdf', 'docx', 'svg', 'png'] as const) {
    expect(() => requireExport(capabilities, format)).toThrow(expect.objectContaining({ code: 'CAPABILITY_UNAVAILABLE' }));
  }
  for (const command of ['open_markdown_file', 'load_local_image', 'resolve_markdown_target', 'export_document', 'render_markdown']) {
    await expect(invokeBrowserCommand(command)).rejects.toMatchObject({ code: 'CAPABILITY_UNAVAILABLE' });
  }
});

test('capabilities are validated rather than inferred from runtime presence', () => {
  for (const value of [null, {}, { ...browserCapabilities, desktopFiles: 1 },
    { ...browserCapabilities, exportFormats: ['pdf', 'pdf'] }, { ...browserCapabilities, exportFormats: ['unknown'] }]) {
    expect(() => parseCapabilities(value)).toThrow();
  }
  const desktop = { ...browserCapabilities, platform: 'linux', desktopFiles: true, exportFormats: ['pdf'] as const };
  expect(() => requireExport(parseCapabilities(desktop), 'pdf')).not.toThrow();
});


test('cancellation acknowledgement is validated and does not imply export completion', async () => {
  for (const status of ['requested', 'notRunning', 'tooLate', 'unsupported']) {
    const client = createTauriClient({ invoke: async () => status });
    await expect(client.cancelExport('job', 'png')).resolves.toBe(status);
  }
  const client = createTauriClient({ invoke: async () => true });
  await expect(client.cancelExport('job', 'png')).rejects.toMatchObject({ code: 'INVALID_RESPONSE' });
});
