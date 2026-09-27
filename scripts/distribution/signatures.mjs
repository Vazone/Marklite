import { spawnSync } from 'node:child_process';
import { join, resolve } from 'node:path';

export function buildVerifier(root) {
  const target = resolve(root, 'local/update-verifier');
  const result = spawnSync('cargo', ['build', '--quiet', '--locked', '--manifest-path',
    'scripts/distribution/verify-signature/Cargo.toml', '--target-dir', target],
  { cwd: root, encoding: 'utf8', windowsHide: true, timeout: 300000 });
  if (result.error || result.status !== 0) throw new Error(`Cannot build signature verifier: ${result.error?.message ?? result.stderr}`);
  return join(target, 'debug', `marklite-verify-signature${process.platform === 'win32' ? '.exe' : ''}`);
}

export function signatureVerifier(executable, publicKey) {
  return async (artifact, signature) => {
    const result = spawnSync(executable, [publicKey, artifact, '-'],
      { input: signature, encoding: 'utf8', windowsHide: true, timeout: 120000 });
    if (result.error) throw result.error;
    return result.status === 0;
  };
}
