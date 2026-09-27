import { findCodeLanguage } from './codeLanguages';
import { canHighlightCode, CODE_LIMITS, type CodeToken } from './codeTokens';

// One owner (preview/editor scheduler) controls concurrency. Every terminal
// path releases its worker, including abort during a local language import.
export function highlightCodeTask(source: string, language: string, signal: AbortSignal,
  createWorker = () => new Worker(new URL('./codeHighlight.worker.ts', import.meta.url), { type: 'module' })
): Promise<CodeToken[]> {
  if (signal.aborted || !canHighlightCode(source) || !findCodeLanguage(language)) return Promise.resolve([]);
  return new Promise(resolve => {
    let worker: Worker | undefined;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let complete = false;
    const finish = (tokens: CodeToken[] = []) => {
      if (complete) return;
      complete = true;
      clearTimeout(timer);
      signal.removeEventListener('abort', abort);
      worker?.terminate();
      resolve(tokens);
    };
    const abort = () => finish();
    signal.addEventListener('abort', abort, { once: true });
    try {
      worker = createWorker();
      // Module startup has a separate deadline; the 100 ms parsing budget
      // begins only after the local language has actually loaded.
      timer = setTimeout(finish, 5000);
      let parsing = false;
      worker.onmessage = (event: MessageEvent<{ ready?: boolean; tokens?: CodeToken[]; error?: string }>) => {
        if (complete) return;
        if (event.data.ready && !parsing) {
          parsing = true;
          clearTimeout(timer);
          timer = setTimeout(finish, CODE_LIMITS.milliseconds);
          worker!.postMessage({ source });
        } else if (event.data.tokens) finish(event.data.tokens);
        else if (event.data.error) {
          console.warn('Code highlighting:', event.data.error);
          finish();
        }
      };
      worker.onerror = event => { event.preventDefault(); finish(); };
      worker.postMessage({ language });
    } catch {
      finish();
    }
  });
}
