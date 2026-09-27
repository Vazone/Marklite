import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { lstat, readFile } from 'node:fs/promises';
import { join } from 'node:path';

export const updaterTargets = Object.freeze({
  'windows-x86_64': { target: 'x86_64-pc-windows-msvc', suffix: '_x64-setup.exe' },
  'linux-x86_64': { target: 'x86_64-unknown-linux-gnu', suffix: '_linux_x86_64.AppImage' },
  'darwin-aarch64': { target: 'aarch64-apple-darwin', suffix: '_macos_aarch64.app.tar.gz' },
  'darwin-x86_64': { target: 'x86_64-apple-darwin', suffix: '_macos_x86_64.app.tar.gz' }
});

// Apply only after the complete staging directory has passed verification.
export function isPublicAsset(name) {
  return name === 'latest.json' || !/\.(?:json|sig)$/i.test(name);
}

export function validateRelease(release) {
  if (!/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(release.version)) throw new Error('Expected stable x.y.z version');
  if (!/^[a-f0-9]{40}$/.test(release.commit)) throw new Error('Expected full source commit');
  if (release.repository !== 'Vazone/Marklite') throw new Error('Unexpected release repository');
  if (typeof release.notes !== 'string' || !release.notes.trim()) throw new Error('Explicit release notes are required');
  if (!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/.test(release.publishedAt) ||
      new Date(release.publishedAt).toISOString().replace('.000Z', 'Z') !== release.publishedAt) throw new Error('Expected valid UTC publication date');
}

async function regular(root, name, maximum = Infinity) {
  // Every caller supplies an exact platform-derived basename, never an arbitrary path.
  const path = join(root, name);
  const info = await lstat(path);
  if (!info.isFile() || info.isSymbolicLink() || info.size === 0 || info.size > maximum) throw new Error(`Invalid release input: ${name}`);
  return path;
}

export async function artifactDigest(root, name) {
  const path = await regular(root, name);
  const hash = createHash('sha256');
  let bytes = 0;
  for await (const chunk of createReadStream(path)) { hash.update(chunk); bytes += chunk.length; }
  return { sha256: hash.digest('hex'), bytes };
}

export async function buildManifest(release, root, verifySignature) {
  validateRelease(release);
  if (typeof verifySignature !== 'function') throw new Error('Cryptographic signature verifier is required');
  const records = release.artifacts;
  if (!Array.isArray(records) || records.length !== Object.keys(updaterTargets).length) throw new Error('Complete four-platform artifact set is required');
  const platforms = {};
  const evidence = [];
  for (const [platform, spec] of Object.entries(updaterTargets)) {
    const matching = records.filter(record => record.platform === platform);
    if (matching.length !== 1) throw new Error(`Missing or duplicate platform: ${platform}`);
    const record = matching[0];
    const name = `MarkLite_${release.version}${spec.suffix}`;
    if (record.commit !== release.commit || record.version !== release.version || record.target !== spec.target || record.name !== name) {
      throw new Error(`Source, version or architecture mismatch: ${platform}`);
    }
    const digest = await artifactDigest(root, name);
    const hash = digest.sha256;
    if (record.sha256 !== hash || record.bytes !== digest.bytes) throw new Error(`Artifact bytes mismatch: ${platform}`);
    const signature = (await readFile(await regular(root, `${name}.sig`, 16384))).toString('utf8').trim();
    if (!signature || signature.length > 16384) throw new Error(`Invalid signature size: ${platform}`);
    if (await verifySignature(join(root, name), signature) !== true) throw new Error(`Invalid signature: ${platform}`);
    platforms[platform] = { signature,
      url: `https://github.com/${release.repository}/releases/download/v${release.version}/${name}` };
    evidence.push({ platform, target: spec.target, name, bytes: digest.bytes, sha256: hash, commit: release.commit });
  }
  return { manifest: { version: release.version, notes: release.notes, pub_date: release.publishedAt, platforms },
    evidence: { schemaVersion: 1, version: release.version, commit: release.commit, artifacts: evidence } };
}
