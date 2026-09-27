import { existsSync, readFileSync } from 'node:fs';
import { delimiter, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

export const root = fileURLToPath(new URL('../../', import.meta.url));
export const toolchain = JSON.parse(readFileSync(new URL('./toolchain.json', import.meta.url), 'utf8'));
export const windows = process.platform === 'win32';

export function androidEnvironment() {
  const local = join(root, 'local/android');
  const sdk = resolve(process.env.ANDROID_HOME || join(local, 'sdk'));
  const java = resolve(process.env.JAVA_HOME || join(local, 'jdk', toolchain.localJdkDirectory));
  const ndk = resolve(process.env.NDK_HOME || join(sdk, 'ndk', toolchain.ndk));
  const javaBinary = join(java, 'bin', windows ? 'java.exe' : 'java');
  for (const path of [javaBinary, join(ndk, 'source.properties'), join(sdk, 'platforms', `android-${toolchain.sdk}`, 'android.jar')]) {
    if (!existsSync(path)) throw new Error(`Missing Android toolchain input: ${path}. Set ANDROID_HOME, JAVA_HOME and NDK_HOME to the versions pinned in scripts/android/toolchain.json.`);
  }
  const version = spawnSync(javaBinary, ['-version'], { encoding: 'utf8', windowsHide: true });
  if (version.status !== 0 || !`${version.stdout}${version.stderr}`.includes(`"${toolchain.jdk.split('.')[0]}.`)) {
    throw new Error(`Expected JDK ${toolchain.jdk.split('.')[0]}; set JAVA_HOME to a supported JDK.`);
  }
  if (!readFileSync(join(ndk, 'source.properties'), 'utf8').includes(`Pkg.Revision = ${toolchain.ndk}`)) {
    throw new Error(`Expected NDK ${toolchain.ndk}; check NDK_HOME.`);
  }
  const env = { ...process.env };
  const pathKey = Object.keys(env).find(key => key.toLowerCase() === 'path') || 'PATH';
  const previousPath = env[pathKey] || '';
  delete env[pathKey];
  return {
    ...env,
    JAVA_HOME: java,
    ANDROID_HOME: sdk,
    ANDROID_SDK_ROOT: sdk,
    NDK_HOME: ndk,
    GRADLE_USER_HOME: process.env.GRADLE_USER_HOME || join(local, 'gradle'),
    ANDROID_USER_HOME: process.env.ANDROID_USER_HOME || join(local, 'user'),
    PATH: [join(java, 'bin'), join(sdk, 'platform-tools'), previousPath].join(delimiter)
  };
}

export function run(command, args, env, cwd = root) {
  const result = spawnSync(command, args, { cwd, env, stdio: 'inherit', windowsHide: true });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${command} exited with ${result.status ?? result.signal}`);
}
