import { markdownParser } from './markdownParser';
import { packMarkdownTree } from './markdownTree';

self.onmessage = (event: MessageEvent<string>) => {
  try {
    self.postMessage({ tree: packMarkdownTree(markdownParser.parse(event.data)) });
  } catch (error) {
    self.postMessage({ error: error instanceof Error ? error.message : 'Markdown parsing failed' });
  }
};
