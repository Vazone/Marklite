import { highlightCodeTask } from './codeHighlightTask';
import type { CodeToken } from './codeTokens';

export function createCodeHighlightCache(run = highlightCodeTask) {
  const entries = new Map<string, { tokens: CodeToken[]; bytes: number }>();
  let bytes = 0;
  return {
    async get(source: string, language: string, signal: AbortSignal) {
      const key = `${language}\0${source}`;
      const cached = entries.get(key);
      if (cached) {
        entries.delete(key);
        entries.set(key, cached);
        return cached.tokens;
      }
      const tokens = await run(source, language, signal);
      if (signal.aborted) return [];
      const size = key.length * 2 + tokens.reduce((sum, token) => sum + 32 + token.classes.length * 2, 0);
      while (entries.size && (entries.size >= 128 || bytes + size > 1024 * 1024)) {
        const oldest = entries.keys().next().value!;
        bytes -= entries.get(oldest)!.bytes;
        entries.delete(oldest);
      }
      if (size <= 1024 * 1024) { entries.set(key, { tokens, bytes: size }); bytes += size; }
      return tokens;
    },
    clear() { entries.clear(); bytes = 0; },
    usage: () => ({ entries: entries.size, bytes })
  };
}
