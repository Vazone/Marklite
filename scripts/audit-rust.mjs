import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { tmpdir } from 'node:os';
import { dirname, join, resolve, basename } from 'node:path';
import { fileURLToPath } from 'node:url';
import { auditRust, verifyDatabase, DATABASE_URL } from './audit/rust.mjs';
import { checkRegistry } from './audit/registry.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
function run(command, args, allowed = [0]) {
  const result = spawnSync(command, args, {
    cwd: root, encoding: 'utf8', timeout: 60_000, maxBuffer: 32 * 1024 * 1024,
    windowsHide: true
  });
  if (result.error) throw new Error(`${command}: ${result.error.code ?? result.error.message}`);
  if (!allowed.includes(result.status)) {
    throw new Error(`${command} exited ${result.status}: ${result.stderr.trim().slice(0, 2000)}`);
  }
  return result;
}

let ownedDirectory;
try {
  const lockHash = () => createHash('sha256').update(readFileSync(join(root, 'src-tauri/Cargo.lock'))).digest('hex');
  const before = lockHash();
  const { report, exitCode } = await auditRust({
    run,
    database(run) {
      process.stderr.write('Verifying current official RustSec database...\n');
      const supplied = process.env.MARKLITE_RUSTSEC_DB;
      if (supplied) return verifyDatabase(run, resolve(supplied));
      ownedDirectory = mkdtempSync(join(tmpdir(), 'marklite-rust-audit-'));
      const directory = join(ownedDirectory, 'advisory-db');
      run('git', ['clone', '--depth', '1', DATABASE_URL, directory]);
      return verifyDatabase(run, directory);
    },
    registry(packages) {
      process.stderr.write('Checking every locked crates.io version against the official sparse index...\n');
      return checkRegistry(packages);
    }
  });
  report.lockfileSha256 = before;
  if (lockHash() !== before) {
    report.status = 'incomplete';
    report.errors.push('Cargo.lock changed during the audit; rerun against one revision');
  }
  process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
  process.exitCode = report.status === 'incomplete' ? 2 : exitCode;
} catch (error) {
  process.stdout.write(`${JSON.stringify({ schemaVersion: 1, status: 'incomplete',
    exceptions: [], steps: {}, errors: [error.message] }, null, 2)}\n`);
  process.exitCode = 2;
} finally {
  // Only this invocation's mkdtemp directory is owned; never remove supplied DBs.
  if (ownedDirectory && dirname(resolve(ownedDirectory)) === resolve(tmpdir()) &&
    basename(ownedDirectory).startsWith('marklite-rust-audit-')) {
    try { rmSync(ownedDirectory, { recursive: true, force: true }); }
    catch (error) { process.stderr.write(`Temporary database cleanup failed: ${error.message}\n`); }
  }
}
