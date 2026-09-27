import { createHash } from 'node:crypto';
import { mkdir, mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { parseArgs } from 'node:util';
import { requireIdle } from './native/process.mjs';
import { verifyExports, verifyCancellation } from './native/cli.mjs';
import { verifyScroll } from './native/scroll.mjs';

const root = resolve(import.meta.dirname, '..');
const { values } = parseArgs({ options: {
  executable: { type: 'string', default: 'src-tauri/target/debug/marklite.exe' },
  'diagram-pack': { type: 'string', default: 'src-tauri/target/optional/mermaid-offline-11.17.2.zip' },
  suite: { type: 'string', default: 'all' }
} });
if (!['all', 'cli', 'scroll'].includes(values.suite)) throw new Error('--suite must be all, cli or scroll');
requireIdle();
const executable = resolve(root, values.executable);
const bytes = await readFile(executable);
const evidence = join(root, 'local/verification/0125');
await mkdir(evidence, { recursive: true });
const output = await mkdtemp(join(evidence, 'native-'));
const env = { ...process.env, MARKLITE_BENCHMARK_MODE: '1', MARKLITE_BENCHMARK_DATA_DIR: join(output, 'profile') };
const context = { root, output, executable, launcher: join(dirname(executable), 'marklite-cli.exe'), env,
  diagramPack: resolve(root, values['diagram-pack']) };
const report = { schemaVersion: 1, suite: values.suite, artifact: { sha256: createHash('sha256').update(bytes).digest('hex') },
  status: 'running', checks: {} };
try {
  if (values.suite !== 'scroll') {
    console.log('Checking native HTML/DOCX/PDF exports...');
    report.checks.exports = await verifyExports(context);
    console.log('Checking PDF cancellation and target preservation...');
    report.checks.cancellation = await verifyCancellation(context);
  }
  if (values.suite !== 'cli') {
    const seed = '# Native scroll regression\n\nParagraph **bold** with `code`.\n\n- list item\n- second item\n\n';
    const content = Buffer.alloc(10 * 1024 * 1024);
    const pattern = Buffer.from(seed);
    for (let at = 0; at < content.length; at += pattern.length) pattern.copy(content, at, 0, Math.min(pattern.length, content.length - at));
    const input = join(output, 'scroll-10-mib.md');
    await writeFile(input, content, { flag: 'wx' });
    console.log('Checking fast native wheel scrolling in split and preview layouts...');
    report.checks.scroll = { fixture: { seed, bytes: content.length, sha256: createHash('sha256').update(content).digest('hex') },
      result: await verifyScroll(context, input) };
  }
  report.status = values.suite === 'all' ? 'passed' : 'partial';
} catch (error) {
  report.status = 'failed';
  report.error = String(error);
  process.exitCode = 1;
} finally {
  await writeFile(join(output, 'report.json'), `${JSON.stringify(report, null, 2)}\n`);
  console.log(JSON.stringify({ output, status: report.status, error: report.error }));
}
