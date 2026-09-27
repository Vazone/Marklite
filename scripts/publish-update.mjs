import { execFileSync } from 'node:child_process';
import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { parseArgs } from 'node:util';
import { publish } from './distribution/publish.mjs';
import { assemble } from './distribution/assemble.mjs';
import { buildVerifier, signatureVerifier } from './distribution/signatures.mjs';

const { values } = parseArgs({ options: { directory: { type: 'string' }, release: { type: 'string' }, 'public-key': { type: 'string' }, 'with-android': { type: 'boolean', default: false } } });
if (process.env.MARKLITE_PUBLISH !== 'true' || !values.directory || !values.release || !values['public-key']) throw new Error('Explicit publish gate and --directory/--release/--public-key are required');
const verify = signatureVerifier(buildVerifier(resolve(import.meta.dirname, '..')), resolve(values['public-key']));
const gh = async (args, body) => execFileSync('gh', args, {
  encoding: 'utf8', windowsHide: true, timeout: 600000, maxBuffer: 16 * 1024 * 1024,
  input: body === undefined ? undefined : JSON.stringify(body)
});
await publish(JSON.parse(await readFile(resolve(values.release), 'utf8')), resolve(values.directory), gh,
  (release, directory) => assemble(release, directory, verify, { verifyExisting: true, withAndroid: values['with-android'] }));
