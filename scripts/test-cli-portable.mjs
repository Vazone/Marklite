// Runs against the platform's real public executable. No shell interpolation.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdtempSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';

const executable = path.resolve(process.argv[2]);
const root = mkdtempSync(path.join(os.tmpdir(), 'marklite-cli-portable-'));
const input = path.join(root, '中文 (draft)&.md');
writeFileSync(input, '# Shared CLI\n\n中文 body\n\n| A | B |\n| --- | --- |\n| one | two |');
const observations = [];
function invoke(args, expected = 0, json = true) {
  const child = spawnSync(executable, args, { encoding: 'utf8', shell: false, timeout: 90000, cwd: root });
  assert.ifError(child.error);
  assert.equal(child.status, expected, `${args.join(' ')}: ${child.stdout} ${child.stderr}`);
  observations.push({ args, exitCode: child.status, stdout: child.stdout, stderr: child.stderr });
  if (!json) return child.stdout;
  assert.equal(child.stderr, '');
  assert.equal(child.stdout.trim().split('\n').length, 1);
  const result = JSON.parse(child.stdout);
  assert.equal(result.schemaVersion, 1);
  assert.equal(result.ok, expected === 0);
  return result;
}
assert.match(invoke(['--help'], 0, false), /--export/);
assert.match(invoke(['--version'], 0, false), /marklite-cli \d/);
for (const format of ['html', 'docx', 'pdf']) {
  for (const command of ['--export', 'export']) {
    const output = path.join(root, `${command}-输出.${format}`);
    const args = [command, '--input', input, '--format', format, '--output', output, '--json'];
    const result = invoke(args);
    assert.equal(result.format, format);
    assert.equal(result.bytes, statSync(output).size);
    const bytes = readFileSync(output);
    if (format === 'html') assert.ok(bytes.toString('utf8').endsWith('</html>'));
    if (format === 'docx') assert.equal(bytes.subarray(0, 2).toString(), 'PK');
    if (format === 'pdf') { assert.equal(bytes.subarray(0, 5).toString(), '%PDF-'); assert.ok(bytes.subarray(-64).includes('%%EOF')); }
    assert.equal(invoke(args, 4).error.code, 'EXPORT_TARGET_EXISTS');
    invoke([...args, '--overwrite']);
  }
}
writeFileSync(path.join(root, '-稿件.md'), 'leading');
invoke(['--export', '--format', 'html', '--json', '--', '-稿件.md']);
assert.equal(invoke(['--export', input, '--format', 'html', '--unknown', '--json'], 2).error.code, 'INVALID_ARGUMENT');
assert.equal(invoke(['--export', input, '--input', input, '--format', 'html', '--json'], 2).error.code, 'INVALID_ARGUMENT');
assert.equal(invoke(['--export', 'missing.md', '--format', 'html', '--json'], 3).error.code, 'FILE_NOT_FOUND');
const sha = file => createHash('sha256').update(readFileSync(file)).digest('hex');
const git = (...args) => spawnSync('git', args, { encoding: 'utf8', shell: false }).stdout.trim();
const report = {
  platform: process.platform, release: os.release(), executable, executableSha256: sha(executable),
  cargoLockSha256: sha('src-tauri/Cargo.lock'), cliSourceSha256: sha('src-tauri/src/cli.rs'),
  sourceBaseCommit: git('rev-parse', 'HEAD'), trackedDiffSha256: createHash('sha256').update(git('diff', 'HEAD')).digest('hex'),
  graphics: { LIBGL_ALWAYS_SOFTWARE: process.env.LIBGL_ALWAYS_SOFTWARE ?? null, GALLIUM_DRIVER: process.env.GALLIUM_DRIVER ?? null },
  node: process.version, observations,
};
const reportPath = process.argv[3] ? path.resolve(process.argv[3]) : path.join(root, 'report.json');
writeFileSync(reportPath, JSON.stringify(report, null, 2) + '\n');
console.log(`Portable CLI passed ${observations.length} invocations (${process.platform}); report: ${reportPath}`);
