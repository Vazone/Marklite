import { readFileSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';

const [sourcePath, licensePath, mode] = process.argv.slice(2);
if (!sourcePath || !licensePath || (mode && mode !== '--check') || process.argv.length > 5) {
  throw new Error('Usage: node scripts/generate-emoji-registry.mjs GEMOJI_JSON GEMOJI_LICENSE [--check]');
}
const source = readFileSync(sourcePath), license = readFileSync(licensePath);
const hash = value => createHash('sha256').update(value).digest('hex');
const sourceSha256 = 'b174ae2aeb321b52f64adb9ff412f966a7f338839d780784dd15dcad702c2dd6';
const licenseSha256 = '7ea1d1fd0602e6623b9a65e791499a0ea5a2f5f60f91bfad78cbaeee19da26e6';
if (hash(source) !== sourceSha256 || hash(license) !== licenseSha256) {
  throw new Error('Input does not match the pinned gemoji v4.1.0 source/license');
}
const aliases = new Map();
for (const row of JSON.parse(source.toString('utf8'))) {
  if (typeof row.emoji !== 'string' || !row.emoji || !Array.isArray(row.aliases)) throw new Error('Invalid upstream emoji');
  for (const name of row.aliases) {
    if (!/^[a-z0-9_+-]+$/.test(name) || aliases.has(name)) throw new Error(`Invalid or duplicate alias: ${name}`);
    aliases.set(name, row.emoji);
  }
}
const registry = {
  version: 'gemoji-4.1.0',
  source: 'https://raw.githubusercontent.com/github/gemoji/v4.1.0/db/emoji.json',
  sourceSha256,
  licenseId: 'MIT',
  license: license.toString('utf8'),
  maxShortcodeLength: Math.max(...[...aliases.keys()].map(name => name.length)),
  aliases: Object.fromEntries([...aliases].sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0))
};
const output = JSON.stringify(registry, null, 2) + '\n';
const destination = fileURLToPath(new URL('../src/shared/emoji-registry.json', import.meta.url));
if (mode === '--check') {
  if (readFileSync(destination, 'utf8') !== output) throw new Error('Generated registry differs from pinned inputs');
} else writeFileSync(destination, output);
console.log(JSON.stringify({ version: registry.version, aliases: aliases.size, bytes: Buffer.byteLength(output), sha256: hash(output), checked: mode === '--check' }));
