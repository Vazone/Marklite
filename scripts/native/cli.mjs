import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdir, readFile, readdir, writeFile } from 'node:fs/promises';
import { join, dirname } from 'node:path';
import { run, successful } from './process.mjs';

const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');

export async function verifyExports(context) {
  const { root, output, launcher, env, diagramPack } = context;
  const profile = env.MARKLITE_BENCHMARK_DATA_DIR;
  const runtime = join(profile, 'diagram-runtime', '11.17.2');
  await mkdir(runtime, { recursive: true });
  const names = ['manifest.json', 'mermaid.min.js', 'LICENSE.mermaid', 'THIRD_PARTY_NOTICES.json'];
  const listing = await successful('tar.exe', ['-tf', diagramPack], { cwd: root });
  assert.deepEqual(listing.stdout.trim().split(/\r?\n/).sort(), [...names].sort(), 'unexpected optional pack entries');
  // Extract known text payloads to stdout, never let archive paths write the filesystem.
  // The application additionally validates manifest sizes and SHA-256 before loading.
  for (const name of names) {
    const payload = await successful('tar.exe', ['-xOf', diagramPack, name], { cwd: root });
    await writeFile(join(runtime, name), payload.stdout, { flag: 'wx' });
  }
  const source = await readFile(join(root, 'scripts/fixtures/math-export.md'));
  const input = join(output, 'diagrams.md');
  await writeFile(input, source, { flag: 'wx' });
  const results = [];
  for (const format of ['html', 'docx', 'pdf']) {
    const path = join(output, `diagrams.${format}`);
    const result = await run(launcher, ['export', input, '--format', format, '--output', path, '--json'], { cwd: root, env });
    assert.equal(result.code, 0, `${format}: ${result.stderr || result.stdout}`);
    const json = JSON.parse(result.stdout);
    assert.equal(json.ok, true, `${format}: unsuccessful JSON result`);
    assert.equal(result.stderr, '', `${format}: JSON export polluted stderr`);
    const bytes = await readFile(path);
    results.push({ format, bytes: bytes.length, sha256: sha256(bytes), result: json });
  }
  const verification = await successful(process.execPath, ['scripts/check-export-artifacts.mjs', '--dir', output, '--formats', 'html,docx,pdf'],
    { cwd: root, env, timeout: 600000 });
  return { fixture: { path: 'scripts/fixtures/math-export.md', sha256: sha256(source) },
    diagramPack: { sha256: sha256(await readFile(diagramPack)) }, results, verification: verification.stdout };
}

export async function verifyCancellation(context) {
  const { root, output, launcher, env } = context;
  const input = join(output, 'cancel.md');
  const target = join(output, 'cancel.pdf');
  const stdout = join(output, 'cancel.stdout.json');
  const stderr = join(output, 'cancel.stderr.txt');
  const helper = join(output, 'cancel-helper.exe');
  const seed = '# cancel\n\n';
  const source = Buffer.from(seed + 'content line\n'.repeat(500000));
  await writeFile(input, source, { flag: 'wx' });
  await writeFile(target, 'keep-cancel', { flag: 'wx' });
  await successful('rustc', ['--edition=2021', 'scripts/fixtures/windows-cli-cancel.rs', '-o', helper], { cwd: root });
  const result = await run(helper, [launcher, dirname(target), stdout, stderr, 'export', '--input', input,
    '--format', 'pdf', '--output', target, '--overwrite', '--json'], { cwd: root, env, timeout: 45000 });
  assert.equal(result.code, 130, result.stderr);
  const json = JSON.parse(await readFile(stdout, 'utf8'));
  assert.equal(json.error.code, 'PDF_EXPORT_CANCELLED');
  assert.equal(await readFile(stderr, 'utf8'), '');
  assert.equal(await readFile(target, 'utf8'), 'keep-cancel');
  assert.equal((await readdir(output)).filter(name => name.startsWith('.marklite-pdf-work-')).length, 0);
  return { fixture: { seed, repeat: 'content line\n', count: 500000, bytes: source.length, sha256: sha256(source) },
    code: result.code, error: json.error.code, targetPreserved: true, workspaceRemoved: true };
}
