import { copyFileSync, createReadStream, existsSync, mkdirSync, mkdtempSync, realpathSync, unlinkSync, rmdirSync, symlinkSync, writeFileSync } from 'node:fs';
import { join, relative, isAbsolute } from 'node:path';
import { parseArgs } from 'node:util';
import { createHash } from 'node:crypto';
import { androidEnvironment, root, run, toolchain, windows } from './environment.mjs';

const { values } = parseArgs({ options: { target: { type: 'string' }, debug: { type: 'boolean', default: false }, optimized: { type: 'boolean', default: false } } });
const arch = { aarch64: 'arm64', x86_64: 'x86_64' }[values.target];
if (!arch) throw new Error('Use --target aarch64 or --target x86_64.');
const abi = arch === 'arm64' ? 'arm64-v8a' : 'x86_64';
if (values.debug && values.optimized) throw new Error('Choose either --debug or --optimized.');
const optimized = values.optimized;
const env = androidEnvironment();
if (windows) {
  const local = join(root, 'local/android');
  mkdirSync(local, { recursive: true });
  const probe = mkdtempSync(join(local, 'symlink-check-'));
  const target = join(probe, 'target');
  const link = join(probe, 'link');
  writeFileSync(target, '');
  try {
    symlinkSync(target, link, 'file');
    unlinkSync(link);
  } catch (cause) {
    throw new Error('Tauri Android packaging requires Windows Developer Mode for symbolic links. Enable it in Windows Settings and retry.', { cause });
  } finally {
    unlinkSync(target);
    rmdirSync(probe);
  }
}
// Gradle's incremental APK writer retained hundreds of MiB of unreferenced
// local headers in a measured build. Clean only this verified generated app
// directory before packaging, then inspect the signed output below.
const androidDir = realpathSync(join(root, 'src-tauri/gen/android'));
const appDir = realpathSync(join(androidDir, 'app'));
const inWorkspace = (path) => {
  const part = relative(realpathSync(root), path);
  return part && part !== '..' && !part.startsWith(`..${process.platform === 'win32' ? '\\' : '/'}`) && !isAbsolute(part);
};
if (!inWorkspace(appDir)) throw new Error('Android app directory is outside the workspace');
const appBuild = join(appDir, 'build');
if (existsSync(appBuild) && !inWorkspace(realpathSync(appBuild))) throw new Error('Android build output is outside the workspace');
run(join(env.JAVA_HOME, 'bin', windows ? 'java.exe' : 'java'),
  ['-classpath', join(androidDir, 'gradle/wrapper/gradle-wrapper.jar'), 'org.gradle.wrapper.GradleWrapperMain', ':app:clean', '--console=plain'], env, androidDir);
run(process.execPath, [join(root, 'node_modules/@tauri-apps/cli/tauri.js'), 'android', 'build',
  '--target', values.target, ...(optimized ? [] : ['--debug']), '--apk', '--split-per-abi', '--ci'], env);

const buildTools = join(env.ANDROID_HOME, 'build-tools', toolchain.buildTools);
const destination = join(root, 'release/android', arch);
mkdirSync(destination, { recursive: true });
const name = optimized ? 'marklite-optimized-test.apk' : 'marklite-debug.apk';
const apk = join(destination, name);
const java = join(env.JAVA_HOME, 'bin', windows ? 'java.exe' : 'java');
const apksigner = join(buildTools, 'lib/apksigner.jar');
const zipalign = join(buildTools, windows ? 'zipalign.exe' : 'zipalign');
let signing = 'gradle-debug';
if (optimized) {
  const releaseDir = join(root, `src-tauri/gen/android/app/build/outputs/apk/${arch}/release`);
  const source = join(releaseDir, `app-${arch}-release-unsigned.apk`);
  if (!existsSync(source)) throw new Error(`Optimized unsigned APK missing: ${source}`);
  const defaultKey = join(root, 'local/android/user/debug.keystore');
  const key = process.env.MARKLITE_ANDROID_TEST_KEYSTORE || defaultKey;
  if (!existsSync(key)) throw new Error('Optimized test signing keystore is missing');
  const keyPassword = process.env.MARKLITE_ANDROID_TEST_KEYSTORE_PASSWORD ||
    (key === defaultKey ? 'android' : null);
  if (!keyPassword) throw new Error('Set MARKLITE_ANDROID_TEST_KEYSTORE_PASSWORD for the external test keystore');
  const alias = process.env.MARKLITE_ANDROID_TEST_KEY_ALIAS || 'androiddebugkey';
  const aligned = join(destination, '.marklite-optimized-aligned.apk');
  try {
    run(zipalign, ['-P', '16', '-f', '4', source, aligned], env);
    const signEnv = { ...env, MARKLITE_ANDROID_SIGNING_PASSWORD: keyPassword,
      MARKLITE_ANDROID_SIGNING_KEY_PASSWORD: process.env.MARKLITE_ANDROID_TEST_KEY_PASSWORD || keyPassword };
    run(java, ['-jar', apksigner, 'sign', '--ks', key, '--ks-key-alias', alias,
      '--ks-pass', 'env:MARKLITE_ANDROID_SIGNING_PASSWORD',
      '--key-pass', 'env:MARKLITE_ANDROID_SIGNING_KEY_PASSWORD', '--out', apk, aligned], signEnv);
  } finally {
    if (existsSync(aligned)) unlinkSync(aligned);
  }
  signing = key === defaultKey ? 'local-debug-test' : 'external-test';
} else {
  const source = join(root, `src-tauri/gen/android/app/build/outputs/apk/${arch}/debug/app-${arch}-debug.apk`);
  copyFileSync(source, apk);
}
run(java, ['-jar', apksigner, 'verify', '--verbose', '--print-certs', apk], env);
run(zipalign, ['-c', '-P', '16', '4', apk], env);
run(process.execPath, [join(root, 'scripts/android/verify.mjs'), apk, abi], env);
const hash = createHash('sha256');
for await (const chunk of createReadStream(apk)) hash.update(chunk);
const sha256 = hash.digest('hex');
writeFileSync(join(destination, `${name}.sha256`), `${sha256}  ${name}\n`);
writeFileSync(join(destination, optimized ? 'build-info-optimized.json' : 'build-info.json'), JSON.stringify({
  package: 'com.marklite.editor', target: values.target, profile: optimized ? 'optimized-test' : 'debug', signing, sha256, toolchain
}, null, 2) + '\n');
console.log(`Android APK: ${apk}\nSHA-256: ${sha256}`);
