import { expect, test } from 'vitest';
import { codeLanguages, findCodeLanguage } from './codeLanguages';
import { canHighlightCode, CODE_LIMITS, parseCodeTokens } from './codeTokens';

const examples: Record<string, string> = {
  JavaScript: 'const greeting = "你好"; // comment\nconsole.log(greeting);',
  TypeScript: 'interface User { name: string }\nconst user: User = { name: "你好" };',
  JSON: '{"name": "你好", "ready": true}',
  HTML: '<div class="greeting">你好</div>',
  CSS: '.greeting { color: red; }',
  Rust: 'fn main() { let name = "你好"; println!("{}", name); }',
  Python: 'def greet(name):\n    return "你好" + name',
  Shell: '#!/bin/bash\necho "$HOME" # comment',
  SQL: "SELECT name FROM users WHERE id = 1;",
  YAML: 'name: 你好\nitems:\n  - true\n'
};

test.each(codeLanguages)('$name produces ordered static ranges without changing text', async language => {
  const source = examples[language.name];
  const support = await language.load();
  const tokens = parseCodeTokens(support.language.parser, source);
  expect(tokens.length).toBeGreaterThan(0);
  let at = 0;
  const pieces: string[] = [];
  for (const token of tokens) {
    expect(token.from).toBeGreaterThanOrEqual(at);
    expect(token.to).toBeGreaterThan(token.from);
    expect(token.to).toBeLessThanOrEqual(source.length);
    expect(token.classes).toMatch(/^tok-[\w-]+(?: tok-[\w-]+)*$/);
    pieces.push(source.slice(at, token.from), source.slice(token.from, token.to));
    at = token.to;
  }
  pieces.push(source.slice(at));
  expect(pieces.join('')).toBe(source);
});

test('language lookup accepts exact local aliases, not fuzzy names or remote paths', () => {
  expect(findCodeLanguage('TS title="x"')?.name).toBe('TypeScript');
  expect(findCodeLanguage('yml')?.name).toBe('YAML');
  for (const name of ['', 'unknown', 'not-javascript', 'https://host/javascript', '<script>']) {
    expect(findCodeLanguage(name)).toBeNull();
  }
});

test('block and long-line budgets reject before invoking the parser', async () => {
  expect(canHighlightCode('x'.repeat(CODE_LIMITS.line))).toBe(true);
  expect(canHighlightCode('x'.repeat(CODE_LIMITS.line + 1))).toBe(false);
  expect(canHighlightCode('x\n'.repeat(CODE_LIMITS.block / 2))).toBe(true);
  const parser = (await findCodeLanguage('js')!.load()).language.parser;
  expect(parseCodeTokens(parser, 'x'.repeat(CODE_LIMITS.line + 1))).toEqual([]);
  expect(parseCodeTokens(parser, 'x\n'.repeat(CODE_LIMITS.block))).toEqual([]);
});
