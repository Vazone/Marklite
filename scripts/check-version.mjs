import { readFileSync } from 'node:fs';

function readJson(path) {
  return JSON.parse(readFileSync(path, 'utf8'));
}

function requireVersion(label, value) {
  if (typeof value !== 'string' || !/^\d+\.\d+\.\d+$/.test(value)) {
    throw new Error(`${label} does not contain a semantic x.y.z version: ${String(value)}`);
  }
  return value;
}

const packageJson = readJson('package.json');
const packageLock = readJson('package-lock.json');
const tauriConfig = readJson('src-tauri/tauri.conf.json');
const cargoToml = readFileSync('src-tauri/Cargo.toml', 'utf8');
const cargoLock = readFileSync('src-tauri/Cargo.lock', 'utf8');

const cargoManifestMatch = cargoToml.match(/^version\s*=\s*"([^"]+)"/m);
const cargoLockMatch = cargoLock.match(
  /\[\[package\]\]\s+name\s*=\s*"marklite"\s+version\s*=\s*"([^"]+)"/
);

const versions = new Map([
  ['package.json', requireVersion('package.json', packageJson.version)],
  ['package-lock.json', requireVersion('package-lock.json', packageLock.version)],
  [
    'package-lock.json root package',
    requireVersion('package-lock.json root package', packageLock.packages?.['']?.version)
  ],
  ['src-tauri/Cargo.toml', requireVersion('src-tauri/Cargo.toml', cargoManifestMatch?.[1])],
  ['src-tauri/Cargo.lock', requireVersion('src-tauri/Cargo.lock', cargoLockMatch?.[1])],
  ['src-tauri/tauri.conf.json', requireVersion('src-tauri/tauri.conf.json', tauriConfig.version)]
]);

const expected = versions.values().next().value;
const mismatches = [...versions].filter(([, version]) => version !== expected);
if (mismatches.length > 0) {
  throw new Error(
    `Version mismatch; expected ${expected}: ${mismatches
      .map(([label, version]) => `${label}=${version}`)
      .join(', ')}`
  );
}

const releaseTag = process.env.RELEASE_TAG ?? process.env.GITHUB_REF_NAME;
if (releaseTag && releaseTag !== `v${expected}`) {
  throw new Error(`Release tag ${releaseTag} does not match application version v${expected}`);
}

console.log(`Version check passed: ${expected}${releaseTag ? ` (${releaseTag})` : ''}`);
