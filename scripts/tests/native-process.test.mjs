import assert from 'node:assert/strict';
import test from 'node:test';
import { run, successful } from '../native/process.mjs';

test('native subprocess preserves split UTF-8, exit status and both streams', async () => {
  const result = await run(process.execPath, ['-e', `
    const bytes = Buffer.from('中文');
    process.stdout.write(bytes.subarray(0, 1));
    setTimeout(() => { process.stdout.write(bytes.subarray(1)); process.stderr.write('error detail'); process.exitCode = 7; }, 20);
  `]);
  assert.equal(result.stdout, '中文');
  assert.equal(result.stderr, 'error detail');
  assert.equal(result.code, 7);
  await assert.rejects(successful(process.execPath, ['-e', 'process.exit(9)']), /failed \(9\)/);
});

test('a hung native subprocess fails on timeout instead of reporting success', async () => {
  await assert.rejects(run(process.execPath, ['-e', 'setInterval(() => {}, 1000)'], { timeout: 100 }), /timed out/);
});
