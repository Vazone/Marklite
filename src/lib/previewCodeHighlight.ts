import { createCodeHighlightCache } from './codeHighlightCache';
import { findCodeLanguage } from './codeLanguages';
import { canHighlightCode, CODE_LIMITS, type CodeToken } from './codeTokens';

function readCode(element: HTMLElement): string | null {
  const walker = element.ownerDocument.createTreeWalker(element, NodeFilter.SHOW_TEXT);
  let source = '';
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    if (source.length + (node.nodeValue?.length ?? 0) > CODE_LIMITS.block) return null;
    source += node.nodeValue ?? '';
  }
  return canHighlightCode(source) ? source : null;
}

export function paintCode(element: HTMLElement, source: string, tokens: CodeToken[]) {
  const fragment = element.ownerDocument.createDocumentFragment();
  let offset = 0;
  for (const token of tokens) {
    fragment.append(source.slice(offset, token.from));
    const span = element.ownerDocument.createElement('span');
    span.className = token.classes;
    span.textContent = source.slice(token.from, token.to);
    fragment.append(span);
    offset = token.to;
  }
  fragment.append(source.slice(offset));
  element.replaceChildren(fragment);
}

export function createPreviewCodeHighlight(cache = createCodeHighlightCache()) {
  let host: HTMLElement | undefined;
  let frame = 0;
  let active = new AbortController();
  let selected: HTMLElement[] = [];
  let disposed = false;
  const painted = new Map<HTMLElement, { source: string; first: ChildNode | null }>();

  function schedule() {
    if (!disposed && !frame) frame = requestAnimationFrame(() => { frame = 0; void refresh(); });
  }
  async function refresh() {
    if (!host || disposed) return;
    const viewport = host.getBoundingClientRect();
    const items: { element: HTMLElement; source: string; language: string }[] = [];
    let units = 0;
    for (const element of host.querySelectorAll<HTMLElement>('pre > code')) {
      const rect = element.getBoundingClientRect();
      if (rect.bottom < viewport.top - viewport.height || rect.top > viewport.bottom + viewport.height) continue;
      const label = Array.from(element.classList).find(value => value.startsWith('language-'))?.slice(9);
      const language = label && findCodeLanguage(label)?.name;
      if (!language) continue;
      const source = readCode(element);
      if (source === null || units + source.length > 262144) continue;
      items.push({ element, source, language });
      units += source.length;
      if (items.length === 64) break;
    }
    const nodes = items.map(item => item.element);
    if (nodes.length === selected.length && nodes.every((node, index) => node === selected[index])) return;
    active.abort();
    active = new AbortController();
    const signal = active.signal;
    selected = nodes;
    for (const element of painted.keys()) {
      if (!nodes.includes(element)) {
        element.textContent = element.textContent;
        painted.delete(element);
      }
    }
    let remaining = CODE_LIMITS.tokens as number;
    for (const { element, source, language } of items) {
      const tokens = await cache.get(source, language, signal);
      if (signal.aborted || !host?.contains(element)) return;
      if (tokens.length > remaining) {
        if (painted.delete(element)) element.textContent = source;
        continue;
      }
      remaining -= tokens.length;
      const previous = painted.get(element);
      if (previous?.source !== source || previous.first !== element.firstChild) {
        paintCode(element, source, tokens);
        painted.set(element, { source, first: element.firstChild });
      }
    }
  }
  return {
    update(next: HTMLElement) {
      active.abort();
      selected = [];
      if (host !== next) {
        host?.removeEventListener('scroll', schedule);
        host = next;
        host.addEventListener('scroll', schedule, { passive: true });
      }
      schedule();
    },
    dispose() {
      disposed = true;
      active.abort();
      cancelAnimationFrame(frame);
      host?.removeEventListener('scroll', schedule);
      host = undefined;
      selected = [];
      painted.clear();
      cache.clear();
    }
  };
}
