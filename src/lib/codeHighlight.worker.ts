import type { Parser } from '@lezer/common';
import { findCodeLanguage } from './codeLanguages';
import { parseCodeTokens } from './codeTokens';

let parser: Parser | undefined;
self.onmessage = async (event: MessageEvent<{ language?: string; source?: string }>) => {
  try {
    if (event.data.language !== undefined) {
      parser = (await findCodeLanguage(event.data.language)?.load())?.language.parser;
      self.postMessage({ ready: true });
    } else if (event.data.source !== undefined) {
      self.postMessage({ tokens: parser ? parseCodeTokens(parser, event.data.source) : [] });
    }
  } catch (error) {
    self.postMessage({ error: error instanceof Error ? error.message : 'Code highlighting failed' });
  }
};
