import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

test('branch dispatch is not a release tag; real and explicit tags still enforce version', () => {
  const root = fileURLToPath(new URL('../../', import.meta.url));
  const { version } = JSON.parse(readFileSync(new URL('../../package.json', import.meta.url)));
  const check = overrides => {
    const env = { ...process.env, GITHUB_REF_TYPE: 'branch', GITHUB_REF_NAME: 'branch' };
    delete env.RELEASE_TAG;
    return spawnSync(process.execPath, ['scripts/check-version.mjs'], { cwd: root, env: { ...env, ...overrides }, encoding: 'utf8', windowsHide: true }).status;
  };
  assert.equal(check({}), 0);
  assert.equal(check({ GITHUB_REF_TYPE: 'tag', GITHUB_REF_NAME: `v${version}` }), 0);
  assert.notEqual(check({ GITHUB_REF_TYPE: 'tag', GITHUB_REF_NAME: 'v999.0.0' }), 0);
  assert.notEqual(check({ RELEASE_TAG: 'v999.0.0' }), 0);
});
