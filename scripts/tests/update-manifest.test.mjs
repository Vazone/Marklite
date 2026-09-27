import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { buildManifest, updaterTargets } from '../distribution/manifest.mjs';

async function fixture(t) {
  const root = await mkdtemp(join(tmpdir(), 'marklite-update-manifest-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  const release = { version: '0.1.7', commit: 'a'.repeat(40), repository: 'Vazone/Marklite',
    publishedAt: '2026-09-24T00:00:00Z', notes: 'Synthetic update fixture', artifacts: [] };
  for (const [platform, spec] of Object.entries(updaterTargets)) {
    const name = `MarkLite_${release.version}${spec.suffix}`;
    const bytes = Buffer.from(`Synthetic ${platform} payload`);
    await writeFile(join(root, name), bytes);
    await writeFile(join(root, `${name}.sig`), 'synthetic-signature');
    release.artifacts.push({ platform, target: spec.target, name, version: release.version, commit: release.commit,
      bytes: bytes.length, sha256: createHash('sha256').update(bytes).digest('hex') });
  }
  return { release, root };
}

test('four platforms have immutable version URLs; no DEB or DMG updater payload', async t => {
  const { release, root } = await fixture(t);
  let verified = 0;
  // This unit verifies the manifest contract only. Real minisign verification is a separate gate.
  const result = await buildManifest(release, root, async () => { verified++; return true; });
  assert.equal(verified, 4);
  assert.equal(Object.keys(result.manifest.platforms).length, 4);
  for (const platform of Object.values(result.manifest.platforms)) {
    assert.match(platform.url, /^https:\/\/github.com\/Vazone\/Marklite\/releases\/download\/v0\.1\.7\/MarkLite_0\.1\.7/);
    assert.doesNotMatch(platform.url, /\.(deb|dmg)$/);
  }
});

test('missing platform, wrong source/architecture, altered bytes and bad signatures fail closed', async t => {
  const { release, root } = await fixture(t);
  await assert.rejects(buildManifest(release, root), /verifier is required/);
  await assert.rejects(buildManifest({ ...release, artifacts: release.artifacts.slice(1) }, root, async () => true), /four-platform/);
  for (const [field, value] of [['commit', 'b'.repeat(40)], ['target', 'aarch64-pc-windows-msvc'], ['name', '../escape.exe'], ['sha256', '0'.repeat(64)]]) {
    const mutated = structuredClone(release);
    mutated.artifacts[0][field] = value;
    await assert.rejects(buildManifest(mutated, root, async () => true), /mismatch/);
  }
  await assert.rejects(buildManifest(release, root, async () => false), /Invalid signature/);
  await assert.rejects(buildManifest({ ...release, repository: 'attacker/other' }, root, async () => true), /repository/);
  await writeFile(join(root, release.artifacts[0].name), 'tampered');
  await assert.rejects(buildManifest(release, root, async () => true), /bytes mismatch/);
});
