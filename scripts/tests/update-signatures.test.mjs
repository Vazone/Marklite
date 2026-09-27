import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { copyFile, mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import test from 'node:test';
import { buildVerifier, signatureVerifier } from '../distribution/signatures.mjs';
import { artifactDigest, updaterTargets } from '../distribution/manifest.mjs';
import { assemble, downloadNames } from '../distribution/assemble.mjs';

const root = resolve(import.meta.dirname, '../..');
const cli = join(root, 'node_modules/@tauri-apps/cli/tauri.js');

test('actual Tauri signatures verify; wrong key, modified payload and modified signature are rejected', async t => {
  const directory = await mkdtemp(join(tmpdir(), 'marklite-test-signatures-'));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const executable = buildVerifier(root);
  const env = { ...process.env };
  for (const name of Object.keys(env)) if (name.startsWith('TAURI_SIGNING_')) delete env[name];
  const call = args => {
    // Tauri key generation may print key material. Never propagate its stdout/stderr.
    const result = spawnSync(process.execPath, [cli, 'signer', ...args], { cwd: root, env, encoding: 'utf8', timeout: 60000, windowsHide: true });
    assert.equal(result.status, 0, `Tauri fixture signer failed: ${args[0]}`);
  };
  const first = join(directory, 'first.key');
  const second = join(directory, 'second.key');
  for (const key of [first, second]) call(['generate', '--ci', '--write-keys', key, '--password', 'fixture-only']);
  const artifact = join(directory, 'payload.bin');
  await writeFile(artifact, Buffer.alloc(1024 * 1024, 0x5a));
  call(['sign', '--private-key-path', first, '--password', 'fixture-only', artifact]);
  const signature = (await readFile(`${artifact}.sig`, 'utf8')).trim();
  const verify = signatureVerifier(executable, `${first}.pub`);
  assert.equal(await verify(artifact, signature), true);
  const assets = join(directory, 'assets');
  await mkdir(assets);
  const release = { version: '0.1.7', commit: 'a'.repeat(40), repository: 'Vazone/Marklite',
    publishedAt: '2026-09-24T00:00:00Z', notes: 'Synthetic signed fixture; not a release' };
  for (const [platform, spec] of Object.entries(updaterTargets)) {
    const name = `MarkLite_${release.version}${spec.suffix}`;
    await copyFile(artifact, join(assets, name));
    await writeFile(join(assets, `${name}.sig`), signature);
    const downloads = [];
    for (const download of downloadNames(platform, release.version)) {
      await writeFile(join(assets, download), `Synthetic download ${download}`);
      downloads.push({ name: download, ...await artifactDigest(assets, download) });
    }
    await writeFile(join(assets, `${platform}.json`), JSON.stringify({ ...release, platform, target: spec.target,
      name, ...await artifactDigest(assets, name), downloads }));
  }
  const firstAsset = join(assets, `MarkLite_${release.version}${updaterTargets['windows-x86_64'].suffix}`);
  await writeFile(`${firstAsset}.sig`, 'broken');
  await assert.rejects(assemble(release, assets, verify), /Invalid signature/);
  await assert.rejects(readFile(join(assets, 'latest.json')), { code: 'ENOENT' });
  await writeFile(`${firstAsset}.sig`, signature);
  const assembled = await assemble(release, assets, verify);
  assert.equal(Object.keys(assembled.manifest.platforms).length, 4);
  assert.equal(assembled.evidence.downloads.length, 5);
  assert.equal((await readFile(join(assets, 'SHA256SUMS.txt'), 'utf8')).trim().split('\n').length, 9);
  await assert.rejects(assemble(release, assets, verify), /Unexpected release asset/);
  await assemble(release, assets, verify, { verifyExisting: true });
  const manifestPath = join(assets, 'latest.json');
  const originalManifest = await readFile(manifestPath, 'utf8');
  await writeFile(manifestPath, originalManifest.replace('https://github.com/', 'https://example.invalid/'));
  await assert.rejects(assemble(release, assets, verify, { verifyExisting: true }), /Assembled metadata mismatch/);
  await writeFile(manifestPath, originalManifest);
  assert.equal(await signatureVerifier(executable, `${second}.pub`)(artifact, signature), false);
  assert.equal(await verify(artifact, 'not a signature'), false);
  const decoded = Buffer.from(signature, 'base64');
  decoded[decoded.length - 8] ^= 1;
  assert.equal(await verify(artifact, decoded.toString('base64')), false);
  await writeFile(artifact, 'modified artifact');
  assert.equal(await verify(artifact, signature), false);
});
