import { createHash } from 'node:crypto';
import { cp, mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { spawn } from 'node:child_process';

const root = resolve(import.meta.dirname, '..');
const runtimeVersion = '11.17.2';
const rendererId = `mermaid-offline-${runtimeVersion}`;
const runtimeRoot = join(root, 'node_modules', 'mermaid');
const output = join(root, 'src-tauri', 'target', 'optional', `${rendererId}.zip`);
const stage = await mkdtemp(join(tmpdir(), 'marklite-mermaid-pack-'));

function sha256(bytes) {
  return createHash('sha256').update(bytes).digest('hex');
}

async function productionDependencyNotices() {
  const lock = JSON.parse(await readFile(join(root, 'package-lock.json'), 'utf8'));
  const packages = lock.packages ?? {};
  const seen = new Set();
  const notices = [];

  function resolveDependency(parentKey, name) {
    let cursor = parentKey;
    while (true) {
      const nested = cursor ? `${cursor}/node_modules/${name}` : `node_modules/${name}`;
      if (packages[nested]) return nested;
      const marker = cursor.lastIndexOf('/node_modules/');
      if (marker < 0) break;
      cursor = cursor.slice(0, marker);
    }
    const topLevel = `node_modules/${name}`;
    if (packages[topLevel]) return topLevel;
    throw new Error(`Cannot resolve locked production dependency ${name} from ${parentKey}`);
  }

  function visit(key, displayName) {
    if (seen.has(key)) return;
    const entry = packages[key];
    if (!entry?.version) throw new Error(`Missing locked package metadata for ${key}`);
    seen.add(key);
    notices.push({
      name: displayName,
      version: entry.version,
      license: entry.license ?? 'SEE PACKAGE METADATA',
      source: entry.resolved ?? null,
      integrity: entry.integrity ?? null
    });
    for (const name of Object.keys(entry.dependencies ?? {}).sort()) {
      visit(resolveDependency(key, name), name);
    }
  }

  visit('node_modules/mermaid', 'mermaid');
  notices.sort((left, right) => left.name.localeCompare(right.name) || left.version.localeCompare(right.version));
  return {
    schemaVersion: 1,
    generatedFrom: 'package-lock.json production dependency closure for mermaid@11.17.2',
    packages: notices
  };
}

function run(command, args) {
  return new Promise((resolveRun, reject) => {
    const child = spawn(command, args, { cwd: root, stdio: 'inherit', shell: false });
    child.once('error', reject);
    child.once('exit', (code, signal) => {
      if (code === 0) resolveRun();
      else reject(new Error(`${command} exited with ${code ?? signal}`));
    });
  });
}

try {
  const packageJson = JSON.parse(await readFile(join(runtimeRoot, 'package.json'), 'utf8'));
  if (packageJson.version !== runtimeVersion) {
    throw new Error(`Expected mermaid ${runtimeVersion}, found ${packageJson.version ?? 'unknown'}`);
  }
  await cp(join(runtimeRoot, 'dist', 'mermaid.min.js'), join(stage, 'mermaid.min.js'));
  await cp(join(runtimeRoot, 'LICENSE'), join(stage, 'LICENSE.mermaid'));
  await writeFile(
    join(stage, 'THIRD_PARTY_NOTICES.json'),
    `${JSON.stringify(await productionDependencyNotices(), null, 2)}\n`,
    'utf8'
  );

  const files = [];
  for (const name of ['mermaid.min.js', 'LICENSE.mermaid', 'THIRD_PARTY_NOTICES.json']) {
    const bytes = await readFile(join(stage, name));
    files.push({ name, bytes: bytes.byteLength, sha256: sha256(bytes) });
  }
  const manifest = {
    schemaVersion: 1,
    generator: 'marklite-build-mermaid-pack-v1',
    rendererId,
    mermaidVersion: runtimeVersion,
    files
  };
  await writeFile(join(stage, 'manifest.json'), `${JSON.stringify(manifest, null, 2)}\n`, 'utf8');
  await mkdir(dirname(output), { recursive: true });
  await run('cargo', [
    'run',
    '--quiet',
    '--manifest-path',
    join(root, 'src-tauri', 'Cargo.toml'),
    '--example',
    'build_mermaid_pack',
    '--features', 'diagram-tooling',
    '--',
    stage,
    output
  ]);
  const archive = await readFile(output);
  console.log(JSON.stringify({ output, bytes: archive.byteLength, sha256: sha256(archive), manifest }, null, 2));
} finally {
  await rm(stage, { recursive: true, force: true });
}
