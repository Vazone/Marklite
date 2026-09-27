import assert from 'node:assert/strict';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { publish } from '../distribution/publish.mjs';
import { artifactDigest } from '../distribution/manifest.mjs';

test('publishing promotes only after remote digests match; errors preserve stable', async t => {
  const directory = await mkdtemp(join(tmpdir(), 'marklite-publish-'));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const release = { version: '0.2.0', commit: 'a'.repeat(40), repository: 'Vazone/Marklite', notes: '中文\nEnglish', publishedAt: '2026-09-24T00:00:00Z' };
  const names = ['latest.json', 'release-evidence.json', 'SHA256SUMS.txt',
    'windows-x86_64.json', 'android-arm64.json', 'MarkLite_0.2.0_x64-setup.exe.release.json',
    'MarkLite_0.2.0_x64-setup.exe.sig', 'MarkLite_0.2.0_x64-setup.exe', 'MarkLite_0.2.0_android_arm64.apk'];
  const publicNames = ['latest.json', 'SHA256SUMS.txt', 'MarkLite_0.2.0_x64-setup.exe', 'MarkLite_0.2.0_android_arm64.apk'];
  for (const name of names) await writeFile(join(directory, name), JSON.stringify({ version: release.version }));
  const remote = [];
  for (const name of publicNames) {
    const digest = await artifactDigest(directory, name);
    remote.push({ name, size: digest.bytes, digest: `sha256:${digest.sha256}`, state: 'uploaded' });
  }
  for (const failure of [null, 'verify', 'upload', 'digest', 'network', 'exists', 'downgrade', 'race', 'tag']) {
    const calls = [];
    let lists = 0;
    const gh = async (args, body) => {
      calls.push({ args, body });
      if (args[1] === 'upload') { if (failure === 'upload') throw new Error('upload failed'); return ''; }
      if (args.includes('PATCH')) return '{}';
      if (args.includes('POST')) return '{"id":42}';
      if (args[1].endsWith('/assets')) return JSON.stringify([failure === 'digest' ? remote.slice(1) : remote]);
      if (args[1].includes('matching-refs')) return JSON.stringify(failure === 'tag' ? [{ ref: 'refs/tags/v0.2.0' }] : []);
      lists++;
      if (failure === 'network') throw new Error('network failed');
      return JSON.stringify([[...(failure === 'exists' ? [{ tag_name: 'v0.2.0', draft: true }] : []),
        ...(failure === 'downgrade' || (failure === 'race' && lists > 1) ? [{ id: 99, tag_name: 'v0.3.0', draft: false, prerelease: false }] : [])]]);
    };
    const verify = async () => { if (failure === 'verify') throw new Error('invalid signature'); };
    if (failure) await assert.rejects(publish(release, directory, gh, verify));
    else await publish(release, directory, gh, verify);
    if (failure === 'verify') assert.equal(calls.length, 0);
    assert.equal(calls.filter(call => call.args.includes('PATCH')).length, failure ? 0 : 1, String(failure));
    if (!failure) {
      const upload = calls.find(call => call.args[1] === 'upload');
      assert.deepEqual(upload.args.slice(3, -2), publicNames.sort().map(name => join(directory, name)));
      assert.deepEqual(calls.at(-1).body, { draft: false, prerelease: false, make_latest: 'true' });
      assert.equal(calls.find(call => call.args.includes('POST')).body.body, release.notes);
    }
  }
  await assert.rejects(publish(release, directory, () => assert.fail('GitHub must not be called without verification')), /verification is required/);
});
