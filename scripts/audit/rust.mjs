const VERSION = 'cargo-audit-audit 0.22.2';
export const DATABASE_URL = 'https://github.com/RustSec/advisory-db.git';

function require(condition, message) {
  if (!condition) throw new Error(message);
}

export function verifyDatabase(run, directory) {
  const git = (...args) => run('git', ['-C', directory, ...args]).stdout.trim();
  // Plain snapshots lose provenance in audit JSON. Require an unchanged checkout
  // matching the official remote's current HEAD, without modifying supplied DBs.
  const root = git('rev-parse', '--show-toplevel');
  const commit = git('rev-parse', 'HEAD');
  require(/^[a-f0-9]{40}$/.test(commit), 'invalid RustSec commit');
  require(git('status', '--porcelain', '--untracked-files=all') === '', 'RustSec snapshot has local changes');
  const remote = run('git', ['ls-remote', DATABASE_URL, 'HEAD']).stdout.trim().split(/\s+/)[0];
  require(commit === remote, 'RustSec snapshot is not the current official HEAD; refresh it before auditing');
  return { directory: root, commit, source: DATABASE_URL, verifiedAt: new Date().toISOString() };
}

function scanReport(result, dependencyCount) {
  const report = JSON.parse(result.stdout);
  require(result.status === 0 || result.status === 1, 'cargo-audit did not complete');
  require(Number.isInteger(report.database?.['advisory-count']) && report.database['advisory-count'] > 0, 'empty or invalid advisory database');
  require(report.lockfile?.['dependency-count'] === dependencyCount, 'audit dependency count differs from locked metadata');
  const settings = report.settings;
  require(settings && Array.isArray(settings.ignore) && settings.ignore.length === 0, 'advisory ignores are forbidden; old quick-xml exceptions were resolved');
  require(Array.isArray(settings.target_arch) && settings.target_arch.length === 0 &&
    Array.isArray(settings.target_os) && settings.target_os.length === 0 && settings.severity === null,
  'audit filters exclude part of the dependency graph');
  require(['unmaintained', 'unsound', 'notice'].every(k => settings.informational_warnings?.includes(k)), 'informational warning classes were disabled');
  const vulnerabilities = report.vulnerabilities;
  require(Array.isArray(vulnerabilities?.list) && vulnerabilities.count === vulnerabilities.list.length &&
    vulnerabilities.found === (vulnerabilities.count > 0), 'invalid vulnerability report');
  require(report.warnings && typeof report.warnings === 'object' && !Array.isArray(report.warnings), 'missing warning report');
  const findings = [];
  for (const [kind, entries] of [['vulnerability', vulnerabilities.list], ...Object.entries(report.warnings)]) {
    require(Array.isArray(entries), 'invalid warning list');
    for (const entry of entries) {
      require(entry.package?.name && entry.package?.version && entry.advisory?.id, 'incomplete advisory finding');
      findings.push({ kind, name: entry.package.name, version: entry.package.version,
        id: entry.advisory.id, title: entry.advisory.title });
    }
  }
  require(result.status === 0 || findings.length > 0, 'nonzero audit exit without findings');
  return { advisoryCount: report.database['advisory-count'], findings };
}

export async function auditRust({ run, database, registry }) {
  const report = { schemaVersion: 1, status: 'incomplete', tool: VERSION, exceptions: [], steps: {}, errors: [] };
  try {
    require(run('cargo', ['audit', '--version']).stdout.trim() === VERSION,
      `requires ${VERSION}: cargo install cargo-audit --locked --version 0.22.2`);
    const metadata = JSON.parse(run('cargo', ['metadata', '--manifest-path', 'src-tauri/Cargo.toml',
      '--format-version', '1', '--locked', '--offline']).stdout);
    require(Array.isArray(metadata.packages) && metadata.packages.length > 0 && metadata.resolve?.root,
      'invalid locked Cargo metadata');
    report.steps.database = database(run);
    const result = run('cargo', ['audit', '--file', 'src-tauri/Cargo.lock', '--json', '--no-yanked',
      '--no-fetch', '--db', report.steps.database.directory], [0, 1]);
    report.steps.advisories = scanReport(result, metadata.packages.length);
    report.steps.registry = await registry(metadata.packages);
    const checked = report.steps.registry;
    require(checked.expected === metadata.packages.filter(p => p.source != null).length &&
      checked.errors.length === 0 && checked.checked === checked.expected,
      'registry checks incomplete; see steps.registry.errors');
    report.status = report.steps.advisories.findings.length || checked.yanked.length ? 'risks_found' : 'passed';
  } catch (error) {
    report.errors.push(error.message);
  }
  return { report, exitCode: { passed: 0, risks_found: 1, incomplete: 2 }[report.status] };
}
