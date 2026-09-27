import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';
import { spawnSync } from 'node:child_process';
import { JSDOM } from 'jsdom';

const { values } = parseArgs({ options: {
  dir: { type: 'string' },
  stem: { type: 'string', default: 'diagrams' },
  formats: { type: 'string', default: 'html,docx,pdf,png' }
} });
assert.ok(values.dir, 'Usage: npm run test:export-artifacts -- --dir <exports> [--stem diagrams] [--formats html,docx,pdf,png]');
const dir = resolve(values.dir);
assert.match(values.stem, /^[^/\\]+$/, 'stem must be a file name');
const formats = values.formats.split(',');
assert.ok(formats.length && formats.every(f => ['html', 'docx', 'pdf', 'png'].includes(f)), 'unknown format');

if (formats.includes('html')) {
  const { document } = new JSDOM(readFileSync(resolve(dir, `${values.stem}.html`), 'utf8')).window;
  assert.equal(document.querySelectorAll('math').length, 10, 'HTML formula count');
  assert.equal(document.querySelectorAll('math mtable').length, 3, 'cases, matrix and align');
  assert.equal(document.querySelectorAll('math mfrac').length, 1, 'fraction');
  assert.equal(document.querySelectorAll('math mroot').length, 1, 'indexed root');
  assert.ok([...document.querySelectorAll('math')].some(n => n.textContent.includes('𝒜')), 'script alphabet');
  assert.ok(!document.body.textContent.includes('PARSE ERROR'), 'math parser error placeholder');
  assert.equal(document.querySelectorAll('svg').length, 2, 'repeated diagrams');
  const links = [...document.querySelectorAll('a[href]')].map(n => n.getAttribute('href'));
  for (const href of ['https://example.com/?a=1&b=2', 'mailto:reader@example.com', '#formula']) {
    assert.ok(links.includes(href), `missing HTML link ${href}`);
  }
  for (const href of links.filter(href => href.startsWith('#'))) {
    assert.ok(document.getElementById(decodeURIComponent(href.slice(1))), `missing HTML target ${href}`);
  }
  console.log('HTML: 10 formulas, 2 diagrams, external and internal links passed');
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const result = spawnSync('cargo', ['run', '--quiet', '--locked', '--manifest-path', 'src-tauri/Cargo.toml',
  '--example', 'check_export_artifacts', '--', dir, values.stem, values.formats], { cwd: root, stdio: 'inherit' });
if (result.error) throw result.error;
process.exitCode = result.status ?? 1;
