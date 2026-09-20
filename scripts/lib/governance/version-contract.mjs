const SEMANTIC_VERSION = /^\d+\.\d+\.\d+$/;

function versionRule(passed, message) {
  return { passed, message };
}

export function readVersionContract(repository) {
  const packageJson = repository.readJson('package.json');
  const packageLock = repository.readJson('package-lock.json');
  const tauriConfig = repository.readJson('src-tauri/tauri.conf.json');
  const cargoManifest = repository.read('src-tauri/Cargo.toml');
  const cargoLock = repository.read('src-tauri/Cargo.lock');
  const cargoVersion = cargoManifest.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
  const cargoLockVersion = cargoLock.match(
    /\[\[package\]\]\s+name\s*=\s*"marklite"\s+version\s*=\s*"([^"]+)"/
  )?.[1];

  return {
    packageJson,
    packageLock,
    tauriConfig,
    cargoManifest,
    cargoLock,
    versions: new Map([
      ['package.json', packageJson.version],
      ['package-lock.json', packageLock.version],
      ['package-lock.json root package', packageLock.packages?.['']?.version],
      ['src-tauri/Cargo.toml', cargoVersion],
      ['src-tauri/Cargo.lock', cargoLockVersion],
      ['src-tauri/tauri.conf.json', tauriConfig.version]
    ])
  };
}

export function evaluateVersionContract(contract, releaseTag, { includePackagePolicy = true } = {}) {
  const rules = [];
  for (const [label, version] of contract.versions) {
    rules.push(
      versionRule(
        typeof version === 'string' && SEMANTIC_VERSION.test(version),
        `${label} does not contain a semantic x.y.z version: ${String(version)}`
      )
    );
  }

  const expected = contract.versions.get('package.json');
  const mismatches = [...contract.versions].filter(([, version]) => version !== expected);
  rules.push(
    versionRule(
      mismatches.length === 0,
      `Version mismatch; expected ${expected}: ${mismatches
        .map(([label, version]) => `${label}=${version}`)
        .join(', ')}`
    )
  );
  rules.push(
    versionRule(
      !releaseTag || releaseTag === `v${expected}`,
      `Release tag ${releaseTag} does not match application version v${expected}`
    )
  );

  if (includePackagePolicy) {
    const lockRoot = contract.packageLock.packages?.[''];
    rules.push(
      versionRule(
        lockRoot?.name === contract.packageJson.name,
        'package-lock.json root package name does not match package.json'
      )
    );
    rules.push(
      versionRule(
        JSON.stringify(lockRoot?.engines) === JSON.stringify(contract.packageJson.engines),
        'package-lock.json root package engines do not match package.json'
      )
    );
    rules.push(
      versionRule(
        contract.packageJson.private === true,
        'package.json must set private=true to prevent accidental publication'
      )
    );
    rules.push(
      versionRule(
        /^publish\s*=\s*false\s*$/m.test(contract.cargoManifest),
        'src-tauri/Cargo.toml must set publish = false'
      )
    );
  }

  return { expected, rules };
}

export function assertVersionContract(repository, releaseTag) {
  const evaluation = evaluateVersionContract(readVersionContract(repository), releaseTag, {
    includePackagePolicy: false
  });
  const errors = evaluation.rules.filter((rule) => !rule.passed).map((rule) => rule.message);
  if (errors.length > 0) {
    throw new Error(errors.join('\n'));
  }
  return evaluation.expected;
}
