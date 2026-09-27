import assert from 'node:assert/strict';
import { mkdtemp, rm, writeFile, readFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { assemble, downloadNames } from '../distribution/assemble.mjs';
import { artifactDigest, updaterTargets } from '../distribution/manifest.mjs';

test('Android is required explicitly, tied to the source, checksummed, and excluded from desktop updater targets', async t => {
  const dir = await mkdtemp(join(tmpdir(), 'marklite-android-release-'));
  t.after(() => rm(dir, { recursive: true, force: true }));
  const release = { version: '0.1.7', commit: 'a'.repeat(40), repository: 'Vazone/Marklite', notes: 'Fixture', publishedAt: '2026-09-27T00:00:00Z' };
  for (const [platform, spec] of Object.entries(updaterTargets)) {
    const name = `MarkLite_${release.version}${spec.suffix}`;
    await writeFile(join(dir, name), `fixture ${platform}`);
    await writeFile(join(dir, `${name}.sig`), 'fixture-only-signature');
    const downloads = [];
    for (const download of downloadNames(platform, release.version)) {
      await writeFile(join(dir, download), 'fixture download');
      downloads.push({ name: download, ...await artifactDigest(dir, download) });
    }
    await writeFile(join(dir, `${platform}.json`), JSON.stringify({ ...release, name, platform, target: spec.target, downloads, ...await artifactDigest(dir, name) }));
  }
  const verify = async () => true; // Cryptographic verification has separate real-signature tests.
  await assert.rejects(assemble(release, dir, verify, { withAndroid: true }), /ENOENT/);
  const name = `MarkLite_${release.version}_android_arm64.apk`;
  await writeFile(join(dir, name), 'fixture APK');
  const record = { ...release, name, platform: 'android-arm64', abi: 'arm64-v8a', signing: 'apk-verified', ...await artifactDigest(dir, name) };
  await writeFile(join(dir, 'android-arm64.json'), JSON.stringify({ ...record, commit: 'b'.repeat(40) }));
  await assert.rejects(assemble(release, dir, verify, { withAndroid: true }), /Android release artifact mismatch/);
  await writeFile(join(dir, 'android-arm64.json'), JSON.stringify(record));
  const result = await assemble(release, dir, verify, { withAndroid: true });
  assert.equal(Object.keys(result.manifest.platforms).length, 4);
  assert.ok(result.evidence.downloads.some(d => d.name === name));
  assert.ok((await readFile(join(dir, 'SHA256SUMS.txt'), 'utf8')).includes(name));
  assert.ok(!(await readFile(join(dir, 'SHA256SUMS.txt'), 'utf8')).includes('.json'));
  await assemble(release, dir, verify, { withAndroid: true, verifyExisting: true });
  await writeFile(join(dir, name), 'tampered APK');
  await assert.rejects(assemble(release, dir, verify, { withAndroid: true, verifyExisting: true }), /Android release artifact mismatch/);
});
