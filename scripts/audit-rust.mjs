import { spawnSync } from 'node:child_process';
import { statSync } from 'node:fs';

const expectedVersion = 'cargo-audit-audit 0.22.2';
const ignoredUpstreamAdvisories = [
  'RUSTSEC-2026-0194',
  'RUSTSEC-2026-0195'
];

function runCargo(args, capture = false) {
  const result = spawnSync('cargo', args, {
    cwd: new URL('..', import.meta.url),
    encoding: 'utf8',
    stdio: capture ? 'pipe' : 'inherit'
  });
  if (result.error) throw result.error;
  return result;
}

const version = runCargo(['audit', '--version'], true);
if (version.status !== 0 || version.stdout.trim() !== expectedVersion) {
  process.stderr.write(
    `Rust audit requires ${expectedVersion}; install it with: cargo install cargo-audit --locked --version 0.22.2\n`
  );
  process.exit(2);
}

// Fail closed when upstream dependency resolution changes so these narrow
// exceptions cannot silently survive after plist accepts a patched quick-xml.
const dependency = runCargo([
  'tree', '--manifest-path', 'src-tauri/Cargo.toml', '--locked', '--target', 'all',
  '-i', 'quick-xml@0.39.4'
], true);
const tree = `${dependency.stdout}\n${dependency.stderr}`;
if (dependency.status !== 0 || !tree.includes('plist v1.9.0')) {
  process.stderr.write(
    'The pinned quick-xml/plist advisory path changed; remove or re-triage the RustSec exceptions.\n'
  );
  process.exit(2);
}

const auditArgs = ['audit', '--file', 'src-tauri/Cargo.lock'];
for (const advisory of ignoredUpstreamAdvisories) {
  auditArgs.push('--ignore', advisory);
}
const suppliedDb = process.env.MARKLITE_RUSTSEC_DB;
if (suppliedDb) {
  try {
    if (!statSync(suppliedDb).isDirectory()) throw new Error('not a directory');
  } catch {
    process.stderr.write(`MARKLITE_RUSTSEC_DB is not a readable directory: ${suppliedDb}\n`);
    process.exit(2);
  }
  process.stderr.write(`Using caller-supplied RustSec database snapshot: ${suppliedDb}\n`);
  auditArgs.push('--db', suppliedDb, '--no-fetch');
}
const audit = runCargo(auditArgs);
process.exit(audit.status ?? 1);
