import assert from 'node:assert/strict';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { verifyArchitecture } from '../distribution/architecture.mjs';

test('architecture gate reads PE/ELF/Mach-O headers and rejects mislabeled or truncated executables', async t => {
  const directory = await mkdtemp(join(tmpdir(), 'marklite-architecture-'));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const path = join(directory, 'binary');
  const platforms = ['windows-x86_64', 'linux-x86_64', 'darwin-aarch64', 'darwin-x86_64'];
  for (const platform of platforms) {
    const bytes = Buffer.alloc(128);
    if (platform.startsWith('windows-')) {
      bytes.write('MZ'); bytes.writeUInt32LE(64, 60); bytes.writeUInt32LE(0x4550, 64);
      bytes.writeUInt16LE(0x8664, 68); bytes.writeUInt16LE(0x20b, 88);
    } else if (platform.startsWith('linux-')) {
      bytes.writeUInt32BE(0x7f454c46); bytes[4] = 2; bytes[5] = 1; bytes.writeUInt16LE(62, 18);
    } else {
      bytes.writeUInt32LE(0xfeedfacf); bytes.writeUInt32LE(platform === 'darwin-aarch64' ? 0x100000c : 0x1000007, 4); bytes.writeUInt32LE(2, 12);
    }
    await writeFile(path, bytes);
    assert.equal(await verifyArchitecture(path, platform), platform);
    for (const other of platforms.filter(value => value !== platform)) await assert.rejects(verifyArchitecture(path, other), /architecture mismatch/);
    await writeFile(path, bytes.subarray(0, 20));
    await assert.rejects(verifyArchitecture(path, platform), /Truncated/);
  }
});
