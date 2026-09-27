import { resolve } from 'node:path';
import { parseArgs } from 'node:util';
import { stage } from './distribution/stage.mjs';

const { values } = parseArgs({ options: {
  directory: { type: 'string' }, platform: { type: 'string' }, commit: { type: 'string' }, 'public-key': { type: 'string' }
} });
if (!values.directory || !values.platform || !values.commit || !values['public-key']) throw new Error('Required: --directory NEW_DIRECTORY --platform PLATFORM --commit SHA --public-key FILE');
const root = resolve(import.meta.dirname, '..');
const record = await stage({ root, directory: resolve(values.directory), platform: values.platform,
  commit: values.commit, publicKey: resolve(values['public-key']) });
console.log(JSON.stringify(record));
