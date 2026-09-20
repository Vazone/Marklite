import { createHash } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const defaultOutput = resolve(repositoryRoot, 'tmp', 'performance-corpus');
const manifestPath = resolve(repositoryRoot, 'benchmarks', 'performance', 'corpus-manifest.json');

function parseArguments(argv) {
  const outputIndex = argv.indexOf('--output');
  return {
    output: outputIndex >= 0 ? resolve(argv[outputIndex + 1]) : defaultOutput,
    writeManifest: argv.includes('--write-manifest')
  };
}

function repeatToBytes(seed, targetBytes) {
  const seedBytes = Buffer.from(seed, 'utf8');
  const output = Buffer.alloc(targetBytes);
  for (let offset = 0; offset < targetBytes; offset += seedBytes.length) {
    seedBytes.copy(output, offset, 0, Math.min(seedBytes.length, targetBytes - offset));
  }
  return output;
}

function representative(bytes) {
  return repeatToBytes(
    '# MarkLite performance corpus\n\nParagraph with **bold**, _emphasis_, [link](https://example.invalid), `code`, 中文和 emoji 🪶.\n\n- [ ] task\n- item\n\n| A | B |\n| --- | ---: |\n| value | 42 |\n\n```ts\nconst value = 42;\n```\n\n',
    bytes
  );
}

function mixed(bytes) {
  return repeatToBytes(
    '# Mixed preview block\n\nA longer paragraph with **bold**, $x^2 + y^2 = z^2$, and 中文 text.\n\n![Local image](./missing-image.png)\n\n$$\\frac{a}{b} + c$$\n\n- [x] complete\n- second item\n\n| Name | Value |\n| --- | ---: |\n| mixed | 42 |\n\n```text\nconst token = "literal $math$";\n```\n\n',
    bytes
  );
}

function numberedLines(count) {
  const chunks = [];
  for (let index = 0; index < count; index += 1) {
    chunks.push(`${String(index + 1).padStart(6, '0')} line with markdown **value** and SEARCH_TARGET_${index % 97}\n`);
  }
  return Buffer.from(chunks.join(''), 'utf8');
}

function headings(count) {
  const chunks = [];
  for (let index = 0; index < count; index += 1) {
    const level = index % 6 + 1;
    chunks.push(`${'#'.repeat(level)} Heading ${String(index + 1).padStart(6, '0')} branch ${Math.floor(index / 6)}\n`);
  }
  return Buffer.from(chunks.join(''), 'utf8');
}

function scrollMarkers(count) {
  const chunks = [];
  for (let index = 0; index < count; index += 1) {
    chunks.push(`# MARKLITE-SCROLL-${String(index).padStart(5, '0')}\n`);
  }
  return Buffer.from(chunks.join(''), 'utf8');
}

function foldMarkers(groups, childrenPerGroup) {
  const chunks = [];
  let marker = 0;
  for (let group = 0; group < groups; group += 1) {
    chunks.push(`# MARKLITE-SCROLL-${String(marker++).padStart(5, '0')}\n`);
    for (let child = 0; child < childrenPerGroup; child += 1) {
      chunks.push(`## MARKLITE-SCROLL-${String(marker++).padStart(5, '0')}\n`);
    }
  }
  return Buffer.from(chunks.join(''), 'utf8');
}

const cases = [
  ['representative-10-kib.md', 'representative', representative(10 * 1024)],
  ['representative-100-kib.md', 'representative', representative(100 * 1024)],
  ['representative-500-kib.md', 'representative', representative(500 * 1024)],
  ['representative-1-mib.md', 'representative', representative(1024 * 1024)],
  ['representative-5-mib.md', 'representative', representative(5 * 1024 * 1024)],
  ['representative-10-mib.md', 'representative', representative(10 * 1024 * 1024)],
  ['mixed-100-kib.md', 'mixed', mixed(100 * 1024)],
  ['mixed-500-kib.md', 'mixed', mixed(500 * 1024)],
  ['single-line-5-mib.md', 'singleLongLine', repeatToBytes('word SEARCH_TARGET 中文 ', 5 * 1024 * 1024)],
  ['lines-130000.md', 'manyLines', numberedLines(130_000)],
  ['headings-150000.md', 'manyHeadings', headings(150_000)],
  ['dense-search-5-mib.md', 'denseSearch', repeatToBytes('SEARCH_TARGET ', 5 * 1024 * 1024)],
  ['scroll-markers-1000.md', 'scrollMarkers', scrollMarkers(1000)],
  ['scroll-fold-markers-1000.md', 'foldMarkers', foldMarkers(100, 9)]
];

async function main() {
  const options = parseArguments(process.argv.slice(2));
  await mkdir(options.output, { recursive: true });
  const manifest = {
    schemaVersion: 1,
    generator: 'scripts/performance/generate-corpus.mjs',
    files: []
  };

  for (const [name, kind, content] of cases) {
    const path = resolve(options.output, name);
    await writeFile(path, content);
    manifest.files.push({
      name,
      kind,
      bytes: content.byteLength,
      sha256: createHash('sha256').update(content).digest('hex')
    });
  }

  if (options.writeManifest) {
    await mkdir(dirname(manifestPath), { recursive: true });
    await writeFile(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`, 'utf8');
  } else {
    const expected = JSON.parse(await readFile(manifestPath, 'utf8'));
    if (JSON.stringify(expected) !== JSON.stringify(manifest)) {
      throw new Error('Generated corpus does not match benchmarks/performance/corpus-manifest.json');
    }
  }

  process.stdout.write(`${JSON.stringify({ output: options.output, ...manifest }, null, 2)}\n`);
}

await main();
