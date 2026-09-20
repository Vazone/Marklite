import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { closeSync, copyFileSync, mkdirSync, openSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';

assert.equal(process.platform, 'win32');
const root = path.resolve('tmp/cli-console-0069');
mkdirSync(root, { recursive: true });
const launcher = path.resolve(process.argv[2] ?? 'src-tauri/target/release/marklite-cli.exe');
function run(program, args) {
  const result = spawnSync(program, args, { encoding: 'utf8', windowsHide: true, timeout: 30000, maxBuffer: 8 * 1024 * 1024 });
  assert.ifError(result.error);
  return result;
}
const helper = path.join(root, 'console.exe');
assert.equal(run('rustc', ['--edition=2021', 'scripts/fixtures/windows-cli-console.rs', '-o', helper]).status, 0);
const input = path.join(root, '中文 (draft)&.md');
writeFileSync(input, '# 中文\n\nbody');
let cases = 0;
for (const shell of ['direct', 'cmd', 'pwsh']) {
  const output = path.join(root, `${shell}.html`);
  for (const [name, args, code, expected] of [
    ['help', ['--help'], 0, 'USAGE:'],
    ['version', ['--version'], 0, 'marklite-cli 0.1.6'],
    ['error', ['--invalid'], 2, 'INVALID_ARGUMENT:'],
    ...['html', 'docx', 'pdf'].map(format => [
      `export-${format}`, ['export', input, '--format', format, '--output', output.replace(/\.html$/, `.${format}`), '--overwrite'], 0, `.${format}`,
    ]),
    ['json', ['export', input, '--format', 'html', '--output', output, '--overwrite', '--json'], 0, '"ok":true'],
  ]) {
    let program = launcher, commandArgs = args;
    if (shell !== 'direct') {
      const script = path.join(root, `${shell}-${name}.${shell === 'cmd' ? 'cmd' : 'ps1'}`);
      const line = shell === 'cmd'
        ? `@echo off\r\n"${launcher}" ${args.map(v => `"${v}"`).join(' ')}\r\nexit /b %errorlevel%\r\n`
        : `& '${launcher.replaceAll("'", "''")}' ${args.map(v => `'${v.replaceAll("'", "''")}'`).join(' ')}\nexit $LASTEXITCODE\n`;
      writeFileSync(script, shell === 'cmd' ? `@chcp 65001 >nul\r\n${line}` : line);
      program = shell === 'cmd' ? process.env.ComSpec : 'pwsh';
      commandArgs = shell === 'cmd' ? ['/d', '/c', script] : ['-NoProfile', '-File', script];
    }
    const report = path.join(root, `${shell}-${name}.txt`);
    const result = run(helper, [report, program, ...commandArgs]);
    assert.equal(result.status, 0, result.stderr);
    const screen = readFileSync(report, 'utf8');
    assert.ok(screen.startsWith(`exit=${code}\nconsole=true\n`), screen);
    assert.ok(screen.includes(expected), `${shell}/${name}: real console missing ${expected}`);
    if (name.startsWith('export-')) {
      assert.ok(screen.includes('Export completed'), `${shell}/${name}: missing shared terminal progress`);
    }
    if (name === 'json') {
      assert.ok(!screen.includes('Export completed'), `${shell}/${name}: progress polluted JSON output`);
    }
    cases++;
  }
}

// Isolated replacement child exercises transport only, not the export engine.
const transportRoot = path.join(root, 'transport');
mkdirSync(transportRoot, { recursive: true });
const transport = path.join(transportRoot, 'marklite-cli.exe');
copyFileSync(launcher, transport);
assert.equal(run('rustc', ['--edition=2021', 'scripts/fixtures/windows-cli-streams.rs', '-o', path.join(transportRoot, 'marklite.exe')]).status, 0);
const dual = run(transport, []);
assert.equal(dual.status, 37);
assert.equal(dual.stdout, 'stdout 中文 😀\n'.repeat(65536));
assert.equal(dual.stderr, 'stderr 中文 😀\n'.repeat(65536));
const mergedPath = path.join(root, 'merged.txt');
const mergedHandle = openSync(mergedPath, 'w');
try {
  const merged = spawnSync(transport, [], { windowsHide: true, timeout: 30000, stdio: ['ignore', mergedHandle, mergedHandle] });
  assert.ifError(merged.error);
  assert.equal(merged.status, 37);
} finally { closeSync(mergedHandle); }
const merged = readFileSync(mergedPath, 'utf8');
assert.equal(merged.length, dual.stdout.length + dual.stderr.length);
assert.ok(!merged.includes('\uFFFD'), 'merged UTF-8 was split between streams');
for (const char of new Set(dual.stdout + dual.stderr)) {
  const count = text => [...text].filter(value => value === char).length;
  assert.equal(count(merged), count(dual.stdout) + count(dual.stderr), `lost merged character ${char}`);
}
await new Promise((resolve, reject) => {
  const child = spawn(transport, [], { windowsHide: true, shell: false });
  const timeout = setTimeout(() => { child.kill(); reject(new Error('closed consumer deadlocked')); }, 30000);
  child.stdout.once('data', () => child.stdout.destroy());
  let tail = '';
  child.stderr.on('data', bytes => { tail = (tail + bytes.toString('utf8')).slice(-1000); });
  child.on('error', reject);
  child.on('exit', code => {
    clearTimeout(timeout);
    try { assert.equal(code, 6); assert.match(tail, /stdout forwarding failed/); resolve(); } catch (error) { reject(error); }
  });
});
const unit = path.join(root, 'unit.exe');
assert.equal(run('rustc', ['--edition=2021', '--test', 'src-tauri/launcher/marklite-cli.rs', '-o', unit]).status, 0);
assert.equal(run(unit, []).status, 0);
console.log(`Real console cases passed: ${cases}; >1 MiB per stream exact UTF-8, merged output, closed-consumer and split UTF-8 unit checks passed.`);
