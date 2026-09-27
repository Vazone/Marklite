import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
const tests = [
  'src/lib/recoveryCoordinator.test.ts', 'src/lib/platform/recovery.test.ts',
  'src/app/controllers/recoveryController.test.ts', 'src/app/controllers/saveController.test.ts',
  'src/lib/documentSave.test.ts', 'src/lib/exitProtection.test.ts', 'src/lib/sessionCoordinator.test.ts'
];
const commands = [
  [process.execPath, ['node_modules/vitest/vitest.mjs', 'run', ...tests]],
  ...['recovery_service', 'atomic_write', 'file_service'].map(filter =>
    ['cargo', ['test', '--manifest-path', 'src-tauri/Cargo.toml', '--locked', filter]])
];
for (const [command, args] of commands) {
  const result = spawnSync(command, args, { cwd: root, stdio: 'inherit', windowsHide: true });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}
