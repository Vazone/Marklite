import { createHash } from 'node:crypto';
import { createReadStream, existsSync, mkdirSync, readFileSync, readdirSync, renameSync, copyFileSync, rmSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { basename, join, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
const version = JSON.parse(readFileSync(join(root, 'src-tauri/tauri.conf.json'), 'utf8')).version;
const name = `MarkLite_${version}_amd64`;
const output = join(root, 'release/linux/amd64');
const bundle = join(root, 'src-tauri/target/release/bundle');

async function sha256(path) {
  const hash = createHash('sha256');
  for await (const chunk of createReadStream(path)) hash.update(chunk);
  return hash.digest('hex');
}

function soleBundle(folder, extension) {
  const candidates = readdirSync(join(bundle, folder)).filter((file) => file.endsWith(extension));
  if (candidates.length !== 1) throw new Error(`Expected one ${extension} in ${folder}; found ${candidates.length}`);
  return join(bundle, folder, candidates[0]);
}

const adoptExisting = process.argv.length === 3 && process.argv[2] === '--adopt-existing';
if (!adoptExisting && process.argv.length !== 2) throw new Error('Usage: npm run package:linux [-- --adopt-existing]');
if (!adoptExisting) {
  if (process.platform !== 'linux') throw new Error('Run package:linux on a Linux host.');
  const result = spawnSync('npm', ['run', 'tauri', '--', 'build', '--bundles', 'appimage,deb'], {
    cwd: root, stdio: 'inherit', env: process.env,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`Linux Tauri build failed: ${result.status ?? result.signal}`);
}

const artifacts = adoptExisting ? [
  [join(root, 'release/linux', `${name}.AppImage`), `${name}.AppImage`],
  [join(root, 'release/linux', `${name}.deb`), `${name}.deb`],
] : [
  [soleBundle('appimage', '.AppImage'), `${name}.AppImage`],
  [soleBundle('deb', '.deb'), `${name}.deb`],
];
mkdirSync(output, { recursive: true });
const staging = join(output, `.stage-${process.pid}-${Date.now()}`);
mkdirSync(staging);
try {
  const records = [];
  for (const [source, fileName] of artifacts) {
    const destination = join(staging, fileName);
    copyFileSync(source, destination);
    const sourceHash = await sha256(source);
    if ((await sha256(destination)) !== sourceHash) throw new Error(`Staged hash mismatch: ${basename(source)}`);
    writeFileSync(`${destination}.sha256`, `${sourceHash}  ${fileName}\n`);
    records.push({ fileName, sha256: sourceHash });
  }
  writeFileSync(join(staging, 'build-info.json'), JSON.stringify({
    schemaVersion: 1, product: 'MarkLite', version, platform: 'linux', architecture: 'amd64',
    buildMode: adoptExisting ? 'adopted-existing' : 'full', artifacts: records,
  }, null, 2) + '\n');
  for (const file of readdirSync(staging)) {
    const target = resolve(output, file);
    if (!target.startsWith(`${resolve(output)}${sep}`)) throw new Error('Invalid Linux output path');
    rmSync(target, { force: true });
    renameSync(join(staging, file), target);
  }
  console.log(`Linux packages complete: ${output}`);
} finally {
  if (existsSync(staging)) rmSync(staging, { recursive: true, force: true });
}
