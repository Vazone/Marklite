import { spawnSync } from 'node:child_process';
import { copyFile, mkdir, readFile, readdir, writeFile } from 'node:fs/promises';
import { join, dirname, basename } from 'node:path';
import { artifactDigest, updaterTargets } from './manifest.mjs';
import { downloadNames } from './assemble.mjs';
import { buildVerifier, signatureVerifier } from './signatures.mjs';
import { verifyArchitecture } from './architecture.mjs';

async function one(directory, suffix) {
  const names = (await readdir(directory)).filter(name => name.endsWith(suffix));
  if (names.length !== 1) throw new Error(`Expected one ${suffix} artifact in ${directory}`);
  return join(directory, names[0]);
}

export async function stage({ root, directory, platform, commit, publicKey }) {
  const spec = updaterTargets[platform];
  if (!spec || !/^[a-f0-9]{40}$/.test(commit)) throw new Error('Invalid platform/source commit');
  const git = spawnSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8', windowsHide: true });
  if (git.status !== 0 || git.stdout.trim() !== commit) throw new Error('Build checkout does not match frozen source commit');
  if (!process.env.TAURI_SIGNING_PRIVATE_KEY && !process.env.TAURI_SIGNING_PRIVATE_KEY_PATH) throw new Error('Signing key is required');
  const { version } = JSON.parse(await readFile(join(root, 'package.json'), 'utf8'));
  if (!/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(version)) throw new Error('Expected stable package version');
  const prefix = `MarkLite_${version}`;
  const bundle = join(root, 'src-tauri/target', ...(platform.startsWith('darwin-') ? [spec.target] : []), 'release/bundle');
  const binary = platform.startsWith('darwin-')
    ? join(bundle, 'macos/MarkLite.app/Contents/MacOS/marklite')
    : join(dirname(bundle), platform.startsWith('windows-') ? 'marklite.exe' : 'marklite');
  await verifyArchitecture(binary, platform);
  const executable = { name: basename(binary), platform, ...await artifactDigest(dirname(binary), basename(binary)) };
  const source = platform === 'windows-x86_64' ? join(bundle, 'nsis', `${prefix}_x64-setup.exe`)
    : platform === 'linux-x86_64' ? await one(join(bundle, 'appimage'), '.AppImage')
      : await one(join(bundle, 'macos'), '.app.tar.gz');
  const names = downloadNames(platform, version);
  const sources = platform === 'windows-x86_64' ? [`${source}.sha256`, `${source}.release.json`]
    : platform === 'linux-x86_64' ? [await one(join(bundle, 'deb'), '.deb')]
      : [await one(join(bundle, 'dmg'), '.dmg')];
  await mkdir(directory); // A fresh staging directory prevents stale assets from another build.
  const name = `${prefix}${spec.suffix}`;
  const artifact = join(directory, name);
  await copyFile(source, artifact);
  const cli = join(root, 'node_modules/@tauri-apps/cli/tauri.js');
  const signed = spawnSync(process.execPath, [cli, 'signer', 'sign', artifact],
    { cwd: root, encoding: 'utf8', windowsHide: true, timeout: 120000 });
  if (signed.error || signed.status !== 0) throw new Error('Signing final updater artifact failed');
  const signature = (await readFile(`${artifact}.sig`, 'utf8')).trim();
  const verify = signatureVerifier(buildVerifier(root), publicKey);
  if (!await verify(artifact, signature)) throw new Error('Final artifact does not verify against the release public key');
  const downloads = [];
  for (let i = 0; i < names.length; i++) {
    await copyFile(sources[i], join(directory, names[i]));
    downloads.push({ name: names[i], ...await artifactDigest(directory, names[i]) });
  }
  const record = { platform, target: spec.target, version, commit, name,
    ...await artifactDigest(directory, name), executable, downloads };
  await writeFile(join(directory, `${platform}.json`), `${JSON.stringify(record, null, 2)}\n`, { flag: 'wx' });
  return record;
}
