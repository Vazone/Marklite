import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { parseArgs } from 'node:util';
import { assemble } from './distribution/assemble.mjs';
import { buildVerifier, signatureVerifier } from './distribution/signatures.mjs';

const { values } = parseArgs({ options: {
  directory: { type: 'string' }, release: { type: 'string' }, 'public-key': { type: 'string' }, 'verify-existing': { type: 'boolean', default: false }, 'with-android': { type: 'boolean', default: false }
} });
if (!values.directory || !values.release || !values['public-key']) throw new Error('Required: --directory ASSETS --release METADATA_JSON --public-key PUBLIC_KEY_FILE');
const root = resolve(import.meta.dirname, '..');
const release = JSON.parse(await readFile(resolve(values.release), 'utf8'));
const verify = signatureVerifier(buildVerifier(root), resolve(values['public-key']));
const result = await assemble(release, resolve(values.directory), verify, { verifyExisting: values['verify-existing'], withAndroid: values['with-android'] });
console.log(JSON.stringify({ version: result.manifest.version, commit: result.evidence.commit, platforms: Object.keys(result.manifest.platforms) }));
