import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { cp, mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import test from 'node:test';

const source = resolve(import.meta.dirname, '../..');

test('staging signs the final installer bytes and rejects wrong source, key and architecture', async t => {
  const root = await mkdtemp(join(tmpdir(), 'marklite-stage-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  const env = { ...process.env };
  for (const name of Object.keys(env)) if (name.startsWith('TAURI_SIGNING_')) delete env[name];
  const command = (binary, args) => spawnSync(binary, args, { cwd: root, env, encoding: 'utf8', windowsHide: true, timeout: 300000 });
  const git = args => {
    const result = command('git', args);
    assert.equal(result.status, 0, 'Fixture Git operation failed');
    return result.stdout.trim();
  };
  git(['init', '--quiet']);
  git(['-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', '-c', 'commit.gpgsign=false', 'commit', '--allow-empty', '-m', 'Fixture']);
  const commit = git(['rev-parse', 'HEAD']);
  await writeFile(join(root, 'package.json'), JSON.stringify({ version: '0.1.6' }));
  const cli = join(source, 'node_modules/@tauri-apps/cli/tauri.js');
  const proxyDirectory = join(root, 'node_modules/@tauri-apps/cli');
  await mkdir(proxyDirectory, { recursive: true });
  await writeFile(join(proxyDirectory, 'tauri.js'), `require(${JSON.stringify(cli)});`);
  const verifier = 'scripts/distribution/verify-signature';
  await cp(join(source, verifier), join(root, verifier), { recursive: true, filter: path => !path.includes(`${join(verifier, 'target')}`) });
  const keys = [join(root, 'test.key'), join(root, 'wrong.key')];
  for (const key of keys) {
    const result = command(process.execPath, [cli, 'signer', 'generate', '--ci', '--write-keys', key, '--password', 'fixture-only']);
    // Signer output can contain private material; never attach it to assertion output.
    assert.equal(result.status, 0, 'Fixture key generation failed');
  }
  env.TAURI_SIGNING_PRIVATE_KEY_PATH = keys[0];
  env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD = 'fixture-only';
  const bundle = join(root, 'src-tauri/target/release/bundle/nsis');
  await mkdir(bundle, { recursive: true });
  const name = 'MarkLite_0.1.6_x64-setup.exe';
  const payload = Buffer.from('Final customized NSIS fixture bytes; not an installable executable');
  await writeFile(join(bundle, name), payload);
  for (const suffix of ['.sha256', '.release.json']) await writeFile(join(bundle, name + suffix), 'Synthetic packaging evidence');
  const header = Buffer.alloc(128);
  header.write('MZ'); header.writeUInt32LE(64, 60); header.writeUInt32LE(0x4550, 64);
  header.writeUInt16LE(0x8664, 68); header.writeUInt16LE(0x20b, 88);
  const binary = join(root, 'src-tauri/target/release/marklite.exe');
  await writeFile(binary, header);
  const run = (directory, overrides = {}) => command(process.execPath, ['--input-type=module', '-e',
    `import {stage} from ${JSON.stringify(pathToFileURL(join(source, 'scripts/distribution/stage.mjs')).href)}; await stage(${JSON.stringify({ root, directory: join(root, directory), platform: 'windows-x86_64', commit, publicKey: keys[0] + '.pub', ...overrides })});`]);
  assert.notEqual(run('wrong-source', { commit: '0'.repeat(40) }).status, 0);
  assert.notEqual(run('wrong-key', { publicKey: keys[1] + '.pub' }).status, 0);
  const result = run('staged');
  assert.equal(result.status, 0, result.stderr);
  assert.deepEqual(await readFile(join(root, 'staged', name)), payload);
  const record = JSON.parse(await readFile(join(root, 'staged/windows-x86_64.json'), 'utf8'));
  assert.equal(record.commit, commit);
  assert.equal(record.downloads.length, 2);
  assert.equal(record.executable.platform, 'windows-x86_64');
  assert.notEqual(run('staged').status, 0, 'Existing staging directory must not be overwritten');
  header.writeUInt16LE(0x14c, 68);
  await writeFile(binary, header);
  assert.notEqual(run('wrong-architecture').status, 0);
});
