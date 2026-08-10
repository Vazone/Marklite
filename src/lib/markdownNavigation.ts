import type { MarkdownTargetDto } from './tauriApi';

export type MarkdownNavigationHandlers = {
  scrollToFragment: (fragment: string) => void;
  openDocument: (path: string, fragment: string | null) => void;
  confirmExternal: (url: string) => boolean;
  openExternal: (url: string) => Promise<void>;
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
    handlers.openDocument(target.path, target.fragment);
    return true;
  }
  if (confirmExternalLinks && !handlers.confirmExternal(target.url)) {
    return false;
  }
  await handlers.openExternal(target.url);
  return true;
}
