import { readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';

const [input, output] = process.argv.slice(2);
if (!input || !output) {
  throw new Error('Usage: node scripts/performance/extract-markdown-stages.mjs <cargo-test-log> <result-json>');
}

const log = await readFile(resolve(input), 'utf8');
const matches = [...log.matchAll(/MARKLITE_STAGE_RESULT (\{[^\r\n]+\})/g)];
if (matches.length !== 5) {
  throw new Error(`Expected five backend stage results, found ${matches.length}`);
}
const cases = Object.fromEntries(matches.map((match) => {
  const entry = JSON.parse(match[1]);
  if (entry.samples !== entry.stages.length || entry.samples < 1) {
    throw new Error(`Invalid sample count for ${entry.name}`);
  }
  return [entry.name, entry];
}));
const result = { schemaVersion: 1, source: input, cases };
await writeFile(resolve(output), `${JSON.stringify(result, null, 2)}\n`, 'utf8');
process.stdout.write(`${JSON.stringify({ output: resolve(output), cases: Object.keys(cases) }, null, 2)}\n`);
