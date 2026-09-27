import { execFileSync } from 'node:child_process';
import { appendFile, readFile } from 'node:fs/promises';
import { validateRelease } from './distribution/manifest.mjs';

const release = { version: process.env.VERSION, commit: process.env.SOURCE_SHA,
  repository: process.env.GITHUB_REPOSITORY, notes: process.env.NOTES,
  publishedAt: new Date().toISOString().replace(/\.\d{3}Z$/, 'Z') };
validateRelease(release);
if (process.env.GITHUB_REF !== 'refs/heads/branch' || process.env.GITHUB_SHA !== release.commit) throw new Error('Dispatch must select branch and its exact workflow commit');
const head = execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
if (head !== release.commit) throw new Error('Checkout differs from frozen source');
execFileSync('git', ['merge-base', '--is-ancestor', release.commit, 'origin/branch']);
if (JSON.parse(await readFile('package.json', 'utf8')).version !== release.version) throw new Error('Requested version differs from source');
const key = (await readFile('src-tauri/updater.pub', 'utf8')).trim();
if (!key) throw new Error('Production public key must be configured before building');
await appendFile(process.env.GITHUB_OUTPUT, `commit=${release.commit}\nmetadata=${JSON.stringify(release)}\n`);
