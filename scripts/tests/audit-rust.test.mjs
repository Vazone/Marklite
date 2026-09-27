import test from 'node:test';
import assert from 'node:assert/strict';
import { auditRust, verifyDatabase } from '../audit/rust.mjs';
import { checkRegistry } from '../audit/registry.mjs';

const source = 'registry+https://github.com/rust-lang/crates.io-index';
const packages = [{ name: 'sample', version: '1.0.0', source }];
function report() {
  return { database: { 'advisory-count': 100 }, lockfile: { 'dependency-count': 1 },
    settings: { ignore: [], target_arch: [], target_os: [], severity: null,
      informational_warnings: ['unmaintained', 'unsound', 'notice'] },
    vulnerabilities: { found: false, count: 0, list: [] }, warnings: {} };
}
function fixture(change = {}) {
  return {
    run(_command, args) {
      if (args.includes('--version')) return { status: 0, stdout: 'cargo-audit-audit 0.22.2' };
      if (args.includes('metadata')) return { status: 0, stdout: JSON.stringify({ packages, resolve: { root: 'sample' } }) };
      return { status: 0, stdout: JSON.stringify(report()) };
    },
    database: () => ({ directory: 'fixture-db', commit: 'a'.repeat(40) }),
    registry: async () => ({ expected: 1, checked: 1, errors: [], yanked: [] }),
    ...change
  };
}

test('complete clean scan is passed, informational risk remains nonzero', async () => {
  assert.equal((await auditRust(fixture())).exitCode, 0);
  const base = fixture();
  const result = await auditRust(fixture({ run(command, args) {
    if (!args.includes('--json')) return base.run(command, args);
    const json = report();
    json.warnings.unsound = [{ package: packages[0], advisory: { id: 'RUSTSEC-TEST', title: 'Unsound' } }];
    return { status: 0, stdout: JSON.stringify(json) };
  } }));
  assert.equal(result.exitCode, 1);
  assert.equal(result.report.status, 'risks_found');
});

for (const error of ['ENOENT', 'ETIMEDOUT', 'invalid database']) {
  test(`${error} is incomplete, never passed`, async () => {
    const result = await auditRust(fixture({ run() { throw new Error(error); } }));
    assert.equal(result.exitCode, 2);
    assert.equal(result.report.status, 'incomplete');
  });
}

for (const [name, change] of [
  ['hidden ignore', r => r.settings.ignore.push('RUSTSEC-2026-0194')],
  ['filtered platform', r => r.settings.target_os.push('windows')],
  ['changed dependency count', r => r.lockfile['dependency-count']++],
  ['empty database', r => r.database['advisory-count'] = 0],
  ['disabled warnings', r => r.settings.informational_warnings = []]
]) {
  test(`${name} fails closed`, async () => {
    const base = fixture();
    const result = await auditRust(fixture({ run(command, args) {
      if (!args.includes('--json')) return base.run(command, args);
      const json = report(); change(json);
      return { status: 0, stdout: JSON.stringify(json) };
    } }));
    assert.equal(result.exitCode, 2);
  });
}

test('registry timeout stays incomplete even when cargo-audit exits zero', async () => {
  const registry = await checkRegistry(packages, async () => { throw new Error('TimeoutError'); });
  const result = await auditRust(fixture({ registry: async () => registry }));
  assert.equal(result.exitCode, 2);
  assert.equal(result.report.steps.registry.checked, 0);
  assert.equal(result.report.steps.registry.errors[0].message, 'TimeoutError');
});

test('registry accounts for every locked version, including yanked versions', async () => {
  const result = await checkRegistry([...packages, { ...packages[0], version: '1.1.0' }], async url => {
    assert.equal(url, 'https://index.crates.io/sa/mp/sample');
    return { ok: true, text: async () => ['1.0.0', '1.1.0'].map(vers => JSON.stringify({ name: 'sample', vers, yanked: vers === '1.1.0' })).join('\n') };
  });
  assert.equal(result.checked, 2);
  assert.deepEqual(result.yanked, [{ name: 'sample', version: '1.1.0' }]);
  const single = await checkRegistry(packages, async () => ({ ok: true,
    text: async () => JSON.stringify({ name: 'sample', vers: '1.0.0', yanked: true }) }));
  assert.equal((await auditRust(fixture({ registry: async () => single }))).exitCode, 1);
});

test('missing/malformed registry versions are not counted as safe', async () => {
  for (const text of ['not-json', JSON.stringify({ name: 'sample', vers: '2.0.0', yanked: false }),
    JSON.stringify({ name: 'sample', vers: '1.0.0' })]) {
    const result = await checkRegistry(packages, async () => ({ ok: true, text: async () => text }));
    assert.equal(result.checked, 0);
    assert.equal(result.errors.length, 1);
  }
});

test('stale or modified snapshots cannot prove a current official scan', () => {
  for (const dirty of [true, false]) {
    assert.throws(() => verifyDatabase((_command, args) => ({ stdout:
      args.includes('status') ? (dirty ? ' M crates/sample.md' : '') :
        args.includes('ls-remote') ? `${'b'.repeat(40)}\tHEAD` :
          args.includes('--show-toplevel') ? '/fixture' : 'a'.repeat(40)
    }), '/fixture'), dirty ? /local changes/ : /current official HEAD/);
  }
});

test('bad database and invalid JSON do not look like an empty clean report', async () => {
  assert.equal((await auditRust(fixture({ database() { throw new Error('invalid database'); } }))).exitCode, 2);
  const base = fixture();
  assert.equal((await auditRust(fixture({ run(command, args) {
    return args.includes('--json') ? { status: 0, stdout: '{}' } : base.run(command, args);
  } }))).exitCode, 2);
});

test('a reintroduced quick-xml advisory is reported rather than ignored', async () => {
  const base = fixture();
  const result = await auditRust(fixture({ run(command, args) {
    if (!args.includes('--json')) return base.run(command, args);
    assert.ok(!args.includes('--ignore'));
    const json = report();
    json.vulnerabilities = { found: true, count: 1, list: [{
      package: { name: 'quick-xml', version: '0.39.4' },
      advisory: { id: 'RUSTSEC-2026-0194', title: 'Quadratic attribute checking' }
    }] };
    return { status: 1, stdout: JSON.stringify(json) };
  } }));
  assert.equal(result.exitCode, 1);
  assert.deepEqual(result.report.exceptions, []);
  assert.equal(result.report.steps.advisories.findings[0].id, 'RUSTSEC-2026-0194');
});
