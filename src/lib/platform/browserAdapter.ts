import { defaultSettings } from './contracts';
import { browserCapabilities } from './capabilities';

export async function invokeBrowserCommand(
  command: string,
  args?: Record<string, unknown>
): Promise<unknown> {
  if (command === 'get_platform_capabilities') return { ...browserCapabilities, exportFormats: [] };
  if (command === 'get_settings') return defaultSettings;
  if (command === 'get_recent_files' || command === 'clear_missing_recent_files') return [];
  if (command === 'get_session') return { version: 1, paths: [], activePath: null };
  if (command === 'update_session') return args?.session;
  if (command === 'clear_session') return undefined;

  if (
    command === 'record_frontend_startup_event' ||
    command === 'clear_startup_diagnostics' ||
    command === 'cancel_local_image_job'
  ) {
    return undefined;
  }

  if (command === 'mark_frontend_ready') {
    return { launchId: 'browser', webviewVersion: null };
  }

  if (command === 'render_markdown' || command === 'analyze_markdown') {
    throw unavailable('Markdown rendering requires the Tauri desktop runtime; the browser adapter does not emulate the desktop protocol.');
  }

  throw unavailable('This operation requires the Tauri desktop runtime.');
}

function unavailable(message: string): Error & { code: string } {
  return Object.assign(new Error(message), { code: 'CAPABILITY_UNAVAILABLE' });
}
