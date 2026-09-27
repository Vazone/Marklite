import { copyFileSync, readFileSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { join, resolve } from 'node:path';
import { parseArgs } from 'node:util';
import { androidEnvironment, root, run, toolchain, windows } from './environment.mjs';
import { artifactDigest } from '../distribution/manifest.mjs';

const { values } = parseArgs({ options: { apk: { type: 'string' }, directory: { type: 'string' }, commit: { type: 'string' } } });
if (!values.apk || !values.directory || !/^[a-f0-9]{40}$/.test(values.commit)) throw new Error('Required: --apk SIGNED_APK --directory ASSETS --commit SOURCE_SHA');
const env = androidEnvironment();
const apk = resolve(values.apk);
const directory = resolve(values.directory);
const { version } = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'));
run(process.execPath, [join(root, 'scripts/android/verify.mjs'), apk, 'arm64-v8a', '--release'], env);
const aapt = join(env.ANDROID_HOME, 'build-tools', toolchain.buildTools, windows ? 'aapt.exe' : 'aapt');
const manifest = execFileSync(aapt, ['dump', 'badging', apk], { env, encoding: 'utf8', windowsHide: true });
if (!manifest.startsWith("package: name='com.marklite.editor'") || !manifest.includes(`versionName='${version}'`) || manifest.includes('application-debuggable')) {
  throw new Error('APK package, version or release profile mismatch');
}
const name = `MarkLite_${version}_android_arm64.apk`;
copyFileSync(apk, join(directory, name), 1);
writeFileSync(join(directory, 'android-arm64.json'), JSON.stringify({ version, commit: values.commit,
  platform: 'android-arm64', abi: 'arm64-v8a', name, signing: 'apk-verified',
  ...await artifactDigest(directory, name) }, null, 2) + '\n', { flag: 'wx' });
