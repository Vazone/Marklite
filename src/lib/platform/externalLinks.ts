import { openUrl } from '@tauri-apps/plugin-opener';
import type { AppError } from './contracts';
import { createTauriClient } from './tauriClient';
import { isTauriRuntime, runtimeCommandTransport } from './runtime';

const emailClient = createTauriClient(runtimeCommandTransport);

export async function openExternalLink(url: string): Promise<void> {
  const parsed = new URL(url);
  if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') {
    throw {
      code: 'UNSUPPORTED_LINK_SCHEME',
      message: `Unsupported link protocol: ${parsed.protocol}`
    } satisfies AppError;
  }
  if (!isTauriRuntime()) {
    window.open(parsed.toString(), '_blank', 'noopener,noreferrer');
    return;
  }

  await openUrl(parsed.toString());
}

export async function openEmailLink(address: string): Promise<void> {
  if (
    address.length === 0 ||
    address.includes('?') ||
    address.includes('#') ||
    [...address].some((character) => character <= '\u001f' || character === '\u007f')
  ) {
    throw {
      code: 'INVALID_MARKDOWN_TARGET',
      message: 'Invalid email link'
    } satisfies AppError;
  }
  const target = new URL(`mailto:${address}`);
  const at = target.pathname.indexOf('@');
  if (
    target.protocol !== 'mailto:' ||
    at <= 0 ||
    at !== target.pathname.lastIndexOf('@') ||
    at === target.pathname.length - 1
  ) {
    throw {
      code: 'INVALID_MARKDOWN_TARGET',
      message: 'Invalid email link'
    } satisfies AppError;
  }
  if (!isTauriRuntime()) {
    window.open(target.toString(), '_blank', 'noopener,noreferrer');
    return;
  }
  await emailClient.openValidatedEmailLink(address);
}
