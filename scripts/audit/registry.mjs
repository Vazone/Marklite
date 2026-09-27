const CRATES_IO = 'registry+https://github.com/rust-lang/crates.io-index';

function indexPath(name) {
  if (!/^[a-zA-Z0-9_-]+$/.test(name)) throw new Error('invalid registry package name');
  name = name.toLowerCase();
  if (name.length < 3) return `${name.length}/${name}`;
  if (name.length === 3) return `3/${name[0]}/${name}`;
  return `${name.slice(0, 2)}/${name.slice(2, 4)}/${name}`;
}

// cargo-audit 0.22.2 logs some index errors without failing its report.
// Account for every locked version against the official sparse index separately.
export async function checkRegistry(packages, fetchIndex = fetch) {
  const groups = new Map();
  for (const pkg of packages) {
    if (pkg.source == null) continue;
    if (pkg.source !== CRATES_IO) throw new Error(`unsupported registry source for ${pkg.name}`);
    const group = groups.get(pkg.name) ?? [];
    group.push(pkg.version);
    groups.set(pkg.name, group);
  }
  const queue = [...groups];
  const result = { expected: [...groups.values()].flat().length, checked: 0, yanked: [], errors: [] };
  async function worker() {
    while (queue.length && result.errors.length === 0) {
      const [name, versions] = queue.shift();
      try {
        const response = await fetchIndex(`https://index.crates.io/${indexPath(name)}`, {
          signal: AbortSignal.timeout(15_000),
          headers: { 'User-Agent': 'MarkLite dependency audit (https://github.com/Vazone/Marklite)' }
        });
        if (!response.ok) throw new Error(`HTTP ${response.status}`);
        const entries = new Map();
        for (const line of (await response.text()).trim().split('\n')) {
          const entry = JSON.parse(line);
          if (versions.includes(entry.vers)) {
            if (entry.name !== name || typeof entry.yanked !== 'boolean' || entries.has(entry.vers)) {
              throw new Error('invalid or duplicate index version');
            }
            entries.set(entry.vers, entry);
          }
        }
        for (const version of versions) {
          const entry = entries.get(version);
          if (!entry) throw new Error(`locked version ${version} absent from index`);
          result.checked++;
          if (entry.yanked) result.yanked.push({ name, version });
        }
      } catch (error) {
        result.errors.push({ package: name, message: error.message,
          ...(error.cause?.code ? { cause: error.cause.code } : {}) });
      }
    }
  }
  await Promise.all(Array.from({ length: Math.min(6, queue.length) }, worker));
  result.yanked.sort((a, b) => `${a.name}@${a.version}`.localeCompare(`${b.name}@${b.version}`));
  result.errors.sort((a, b) => a.package.localeCompare(b.package));
  return result;
}
