import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { JSDOM } from 'jsdom';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const cache = path.join(os.tmpdir(), 'marklite-markdown-conformance-0086');
const sources = [
  {
    name: 'CommonMark 0.31.2', file: 'commonmark.json',
    url: 'https://spec.commonmark.org/0.31.2/spec.json',
    sha256: 'd431b29d97b6f73e69d547109cf5081578fac931e72afe95639ebe766c1b2a20',
    license: 'CC BY-SA 4.0'
  },
  {
    name: 'GFM 0.29-gfm', file: 'gfm.html',
    url: 'https://github.github.com/gfm/',
    sha256: 'b153d814fdfc8624bb6da7449162c1cd707a637f7d1c1b636eb44b9cf63fa220',
    license: 'CC BY-SA 4.0'
  }
];
const dom = new JSDOM('').window.document;

function normalize(html) {
  const template = dom.createElement('template');
  template.innerHTML = html.replace(/\r\n/g, '\n');
  for (const element of template.content.querySelectorAll('*')) {
    if (element.matches('th[align],td[align]')) {
      element.setAttribute('style', `text-align: ${element.getAttribute('align')}`);
      element.removeAttribute('align');
    }
    const attrs = [...element.attributes].map(({ name, value }) => [name, value]).sort();
    for (const { name } of [...element.attributes]) element.removeAttribute(name);
    for (const [name, value] of attrs) element.setAttribute(name, value);
  }
  for (const body of template.content.querySelectorAll('tbody')) {
    if (body.children.length === 0 && body.textContent.trim() === '') body.remove();
  }
  const walker = dom.createTreeWalker(template.content, 4);
  const textNodes = [];
  while (walker.nextNode()) textNodes.push(walker.currentNode);
  for (const node of textNodes) {
    if (node.parentElement?.matches('table,thead,tbody,tr,th,td,li,ul,ol')) {
      const collapsed = node.data.replace(/\s+/g, ' ');
      if (collapsed === ' ') node.remove();
      else node.data = collapsed;
    }
  }
  return template.innerHTML.trim();
}

async function ensureCorpus() {
  fs.mkdirSync(cache, { recursive: true });
  for (const source of sources) {
    const file = path.join(cache, source.file);
    if (!fs.existsSync(file)) {
      const response = await fetch(source.url);
      if (!response.ok) throw new Error(`${source.url}: HTTP ${response.status}`);
      fs.writeFileSync(file, Buffer.from(await response.arrayBuffer()));
    }
    const digest = createHash('sha256').update(fs.readFileSync(file)).digest('hex');
    if (digest !== source.sha256) throw new Error(`Pinned official corpus digest mismatch: ${source.file}`);
  }
}

function officialCases() {
  const cases = JSON.parse(fs.readFileSync(path.join(cache, 'commonmark.json'), 'utf8'))
    .map((row) => ({ ...row, suite: 'CommonMark 0.31.2' }));
  const gfm = new JSDOM(fs.readFileSync(path.join(cache, 'gfm.html'), 'utf8')).window.document;
  let section = '';
  for (const element of gfm.querySelectorAll('h1,h2,h3,h4,.example')) {
    if (!element.classList.contains('example')) {
      section = element.textContent.trim();
      continue;
    }
    cases.push({
      suite: 'GFM 0.29-gfm', section,
      example: Number(element.id.slice('example-'.length)),
      markdown: element.querySelector('.language-markdown').textContent.replaceAll('→', '\t'),
      html: element.querySelector('.language-html').textContent.replaceAll('→', '\t')
    });
  }
  return cases;
}

function renderWithProduction(cases) {
  const source = path.join(root, 'Review/probes/markdown-syntax.rs').replaceAll('\\', '/');
  const manifest = fs.readFileSync(path.join(root, 'Review/probes/markdown-syntax.Cargo.toml'), 'utf8')
    .replaceAll('__PROBE_SOURCE__', source);
  const work = fs.mkdtempSync(path.join(cache, 'probe-'));
  const manifestPath = path.join(work, 'Cargo.toml');
  fs.writeFileSync(manifestPath, manifest);
  fs.copyFileSync(path.join(root, 'src-tauri/Cargo.lock'), path.join(work, 'Cargo.lock'));
  const casesPath = path.join(work, 'cases.json');
  const renderedPath = path.join(work, 'rendered.json');
  fs.writeFileSync(casesPath, JSON.stringify(cases));
  const run = spawnSync('cargo', [
    'run', '--offline', '--manifest-path', manifestPath,
    '--target-dir', path.join(root, 'src-tauri/target'), '--', casesPath, renderedPath
  ], {
    cwd: root,
    env: { ...process.env, MARKLITE_REVIEW_ROOT: root.replaceAll('\\', '/') },
    stdio: 'inherit'
  });
  if (run.error) throw run.error;
  if (run.status !== 0) throw new Error(`Production Markdown probe failed: ${run.status}`);
  return JSON.parse(fs.readFileSync(renderedPath, 'utf8'));
}

const exceptionContract = JSON.parse(fs.readFileSync(path.join(root, 'scripts/fixtures/markdown-conformance-exceptions.json'), 'utf8'));
if (exceptionContract.schemaVersion !== 1 || exceptionContract.suite !== 'GFM 0.29-gfm') {
  throw new Error('Unsupported conformance exception contract');
}
const classifiedDifferences = new Map(exceptionContract.exceptions.map((entry) => [entry.example, entry]));
if (classifiedDifferences.size !== exceptionContract.exceptions.length) throw new Error('Duplicate GFM exception');

function compare(rows) {
  const cases = [];
  const totals = {};
  const seenDifferences = new Set();
  for (const row of rows) {
    if (row.suite !== 'CommonMark 0.31.2' && row.suite !== 'GFM 0.29-gfm') continue;
    const key = row.suite;
    const expected = normalize(row.expected);
    const actual = normalize(key === 'CommonMark 0.31.2' ? row.commonmark : row.gfm);
    const suite = totals[key] ??= { total: 0, pass: 0, safetyDivergence: 0, dialectDivergence: 0 };
    suite.total++;
    let status = 'pass';
    let reason;
    if (actual !== expected) {
      const difference = key === 'GFM 0.29-gfm' ? classifiedDifferences.get(row.example) : undefined;
      if (!difference) throw new Error(`Unclassified official mismatch: ${key} #${row.example} ${row.section}`);
      const actualSha256 = createHash('sha256').update(actual).digest('hex');
      if (actualSha256 !== difference.actualSha256) {
        throw new Error(`Official exception output changed: ${key} #${row.example}`);
      }
      ({ status, reason } = difference);
      seenDifferences.add(row.example);
    }
    if (status === 'pass') suite.pass++;
    if (status === 'safety-divergence') suite.safetyDivergence++;
    if (status === 'dialect-divergence') suite.dialectDivergence++;
    cases.push({ suite: key, section: row.section, example: row.example, status,
      ...(reason ? { reason, actualSha256: createHash('sha256').update(actual).digest('hex') } : {}) });
  }
  if (totals['CommonMark 0.31.2']?.total !== 652 || totals['GFM 0.29-gfm']?.total !== 677) {
    throw new Error('Rendered official case count is incomplete');
  }
  for (const example of classifiedDifferences.keys()) {
    if (!seenDifferences.has(example)) throw new Error(`Stale GFM exception: ${example}`);
  }
  return { schemaVersion: 1, sources, totals, cases };
}

function capabilityMatrix(official) {
  const contract = JSON.parse(fs.readFileSync(path.join(root, 'src/shared/markdown-capabilities.json'), 'utf8'));
  if (contract.schemaVersion !== 1) throw new Error('Unsupported capability contract version');
  const surfaces = ['editor', 'preview', 'gui', 'cli', 'shell', 'html', 'pdf', 'docx'];
  const allowed = new Set(['source', 'native', 'mapped', 'shared', 'windows', 'filtered', 'degraded', 'conditional', 'highlight-only', 'pending-verification']);
  const validateStatuses = (statuses, label) => {
    if (Object.keys(statuses).sort().join(',') !== [...surfaces].sort().join(',')) {
      throw new Error(`Missing or unknown capability surface: ${label}`);
    }
    for (const value of Object.values(statuses)) {
      if (!allowed.has(value)) throw new Error(`Unknown capability status ${value}: ${label}`);
    }
  };
  const validateOwner = (owner, label) => {
    const ids = owner.match(/\d{4}/g) ?? [];
    if (!ids.length) throw new Error(`Missing TaskCard owner: ${label}`);
    for (const id of ids) {
      const exists = ['active', 'done', 'blocked'].some((folder) =>
        fs.readdirSync(path.join(root, 'TaskCards', folder)).some((file) => file.startsWith(`${id}-`)));
      if (!exists) throw new Error(`Unknown TaskCard ${id}: ${label}`);
    }
  };
  validateStatuses(contract.surfaceDefaults, 'surfaceDefaults');
  const chapters = new Map();
  for (const row of official) {
    const key = `${row.suite}\u0000${row.section}`;
    if (!chapters.has(key)) chapters.set(key, { suite: row.suite, section: row.section, fixtureExample: row.example });
  }
  const overrides = new Map();
  for (const override of contract.chapterOverrides) {
    const key = `${override.suite}\u0000${override.section}`;
    if (!chapters.has(key) || overrides.has(key)) throw new Error(`Stale or duplicate chapter override: ${key}`);
    overrides.set(key, override);
  }
  const matrix = [];
  for (const [key, chapter] of chapters) {
    const override = overrides.get(key) ?? {};
    const statuses = { ...contract.surfaceDefaults, ...override.statuses };
    validateStatuses(statuses, key);
    const owner = override.owner ?? contract.chapterDefaultOwner;
    validateOwner(owner, key);
    matrix.push({ kind: 'official-chapter', ...chapter, owner, statuses });
  }
  const extensionIds = new Set();
  for (const extension of contract.namedExtensions) {
    if (extensionIds.has(extension.id) || !extension.fixture || !extension.boundary) {
      throw new Error(`Duplicate or incomplete named extension: ${extension.id}`);
    }
    extensionIds.add(extension.id);
    validateStatuses(extension.statuses, extension.id);
    validateOwner(extension.owner, extension.id);
    matrix.push({ kind: 'named-extension', ...extension });
  }
  return { profileModes: contract.profiles, matrix };
}

const args = process.argv.slice(2);
if (args.length > 2 || (args[0] && args[0] !== '--output') || (args[0] && !args[1])) {
  throw new Error('Usage: npm run conformance:markdown -- [--output PATH]');
}
await ensureCorpus();
const cases = officialCases();
if (cases.filter((row) => row.suite === 'CommonMark 0.31.2').length !== 652
  || cases.filter((row) => row.suite === 'GFM 0.29-gfm').length !== 677) {
  throw new Error('Official example count changed');
}
const contract = JSON.parse(fs.readFileSync(path.join(root, 'src/shared/markdown-capabilities.json'), 'utf8'));
cases.push(...contract.namedExtensions.map((extension) => ({
  suite: 'MarkLite named extensions', section: extension.id,
  example: extension.id, markdown: extension.fixture, html: ''
})));
const rows = renderWithProduction(cases);
const report = compare(rows);
const capability = capabilityMatrix(cases.filter((row) => row.suite !== 'MarkLite named extensions'));
report.profileModes = capability.profileModes;
report.capabilities = capability.matrix;
const extensionRows = new Map(rows
  .filter((row) => row.suite === 'MarkLite named extensions')
  .map((row) => [row.example, row]));
if (extensionRows.size !== contract.namedExtensions.length) throw new Error('Named extension fixture count mismatch');
report.extensionObservations = contract.namedExtensions.map((extension) => {
  const row = extensionRows.get(extension.id);
  if (!row?.configured.includes(extension.expectedConfiguredContains)
    || (extension.expectedPreviewContains && !row.preview.includes(extension.expectedPreviewContains))) {
    throw new Error(`Named extension fixture failed: ${extension.id}`);
  }
  return {
    id: extension.id,
    status: 'pass',
    configuredSha256: createHash('sha256').update(row.configured).digest('hex'),
    previewSha256: createHash('sha256').update(row.preview).digest('hex')
  };
});
if (args[0] === '--output') {
  const output = path.resolve(args[1]);
  if (fs.existsSync(output)) throw new Error(`Report already exists; choose a new path: ${output}`);
  fs.mkdirSync(path.dirname(output), { recursive: true });
  fs.writeFileSync(output, `${JSON.stringify(report, null, 2)}\n`, { flag: 'wx' });
  console.log(`Report: ${output}`);
}
console.log(JSON.stringify(report.totals));
