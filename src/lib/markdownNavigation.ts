import type { MarkdownTargetDto } from './tauriApi';
import { documentResource, type ResourceRef } from './platform/resources';

export type MarkdownNavigationHandlers = {
  scrollToFragment: (fragment: string) => void;
  openDocument: (resource: ResourceRef, fragment: string | null) => Promise<boolean>;
  confirmExternal: (url: string) => boolean;
  openExternal: (url: string) => Promise<void>;
  openEmail: (address: string) => Promise<void>;
};

export async function executeMarkdownTarget(
  target: MarkdownTargetDto,
  confirmExternalLinks: boolean,
  handlers: MarkdownNavigationHandlers
): Promise<boolean> {
  if (target.kind === 'anchor') {
    handlers.scrollToFragment(target.fragment);
    return true;
  }
  if (target.kind === 'localDocument') {
    const resource = documentResource(target);
    return resource ? handlers.openDocument(resource, target.fragment) : false;
  }
  const externalTarget = target.kind === 'email' ? `mailto:${target.address}` : target.url;
  if (confirmExternalLinks && !handlers.confirmExternal(externalTarget)) {
    return false;
  }
  if (target.kind === 'email') await handlers.openEmail(target.address);
  else await handlers.openExternal(target.url);
  return true;
}
