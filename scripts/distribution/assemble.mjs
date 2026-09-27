import { readFile, readdir, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { artifactDigest, buildManifest, isPublicAsset, updaterTargets, validateRelease } from './manifest.mjs';

export function downloadNames(platform, version) {
  const prefix = `MarkLite_${version}`;
  switch (platform) {
    case 'windows-x86_64': return [`${prefix}_x64-setup.exe.sha256`, `${prefix}_x64-setup.exe.release.json`];
    case 'linux-x86_64': return [`${prefix}_linux_amd64.deb`];
    case 'darwin-aarch64': return [`${prefix}_macos_aarch64.dmg`];
    case 'darwin-x86_64': return [`${prefix}_macos_x86_64.dmg`];
    default: throw new Error('Unknown download platform');
  }
}

export async function assemble(release, directory, verifySignature, { verifyExisting = false, withAndroid = false } = {}) {
  validateRelease(release);
  const records = [];
  const downloads = [];
  for (const platform of Object.keys(updaterTargets)) {
    const record = JSON.parse(await readFile(join(directory, `${platform}.json`), 'utf8'));
    if (record.platform !== platform) throw new Error(`Record platform mismatch: ${platform}`);
    const names = downloadNames(platform, release.version);
    if (!Array.isArray(record.downloads) || record.downloads.length !== names.length) throw new Error(`Missing download assets: ${platform}`);
    for (const name of names) {
      const matching = record.downloads.filter(item => item.name === name);
      if (matching.length !== 1) throw new Error(`Missing or duplicate download asset: ${name}`);
      const digest = await artifactDigest(directory, name);
      if (matching[0].sha256 !== digest.sha256 || matching[0].bytes !== digest.bytes) throw new Error(`Download asset mismatch: ${name}`);
      downloads.push({ name, ...digest });
    }
    records.push(record);
  }
  const allowed = new Set(records.flatMap(record => [record.name, `${record.name}.sig`, `${record.platform}.json`,
    ...(record.downloads ?? []).map(item => item.name)]));
  if (withAndroid) {
    const name = `MarkLite_${release.version}_android_arm64.apk`;
    const record = JSON.parse(await readFile(join(directory, 'android-arm64.json'), 'utf8'));
    const digest = await artifactDigest(directory, name);
    if (record.name !== name || record.platform !== 'android-arm64' || record.abi !== 'arm64-v8a' ||
      record.version !== release.version || record.commit !== release.commit || record.signing !== 'apk-verified' ||
      record.sha256 !== digest.sha256 || record.bytes !== digest.bytes) throw new Error('Android release artifact mismatch');
    allowed.add(name);
    allowed.add('android-arm64.json');
    downloads.push({ name, ...digest });
  }
  if (verifyExisting) for (const name of ['latest.json', 'release-evidence.json', 'SHA256SUMS.txt']) allowed.add(name);
  for (const name of await readdir(directory)) if (!allowed.has(name)) throw new Error(`Unexpected release asset: ${name}`);
  const result = await buildManifest({ ...release, artifacts: records }, directory, verifySignature);
  result.evidence.downloads = downloads;
  const checksums = [...result.evidence.artifacts, ...downloads].filter(item => isPublicAsset(item.name))
    .map(item => `${item.sha256}  ${item.name}`).sort().join('\n') + '\n';
  if (verifyExisting) {
    for (const [name, expected] of Object.entries({ 'latest.json': `${JSON.stringify(result.manifest, null, 2)}\n`,
      'release-evidence.json': `${JSON.stringify(result.evidence, null, 2)}\n`, 'SHA256SUMS.txt': checksums })) {
      if (await readFile(join(directory, name), 'utf8') !== expected) throw new Error(`Assembled metadata mismatch: ${name}`);
    }
    return result;
  }
  // These are local staging writes. No Release/latest pointer exists until the publish job.
  await writeFile(join(directory, 'latest.json'), `${JSON.stringify(result.manifest, null, 2)}\n`, { flag: 'wx' });
  await writeFile(join(directory, 'release-evidence.json'), `${JSON.stringify(result.evidence, null, 2)}\n`, { flag: 'wx' });
  await writeFile(join(directory, 'SHA256SUMS.txt'), checksums, { flag: 'wx' });
  return result;
}
