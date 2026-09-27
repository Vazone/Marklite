import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';
import { androidEnvironment, root, run } from './environment.mjs';
import { artifactDigest } from '../distribution/manifest.mjs';

// Hosted builds never receive an Android private key. Signing is an offline release step.
const env = androidEnvironment();
const { version } = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'));
const commit = execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim();
run(process.execPath, [join(root, 'node_modules/@tauri-apps/cli/tauri.js'), 'android', 'build',
  '--target', 'aarch64', '--apk', '--split-per-abi', '--ci'], env);
const directory = join(root, 'android-unsigned');
mkdirSync(directory);
const name = `MarkLite_${version}_android_arm64-unsigned.apk`;
copyFileSync(join(root, 'src-tauri/gen/android/app/build/outputs/apk/arm64/release/app-arm64-release-unsigned.apk'), join(directory, name));
writeFileSync(join(directory, 'android-unsigned.json'), JSON.stringify({
  version, commit, name, abi: 'arm64-v8a', signing: 'unsigned', ...await artifactDigest(directory, name)
}, null, 2) + '\n');
