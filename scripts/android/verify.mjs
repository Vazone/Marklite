import { closeSync, openSync, readSync, statSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { androidEnvironment, run, toolchain, windows } from './environment.mjs';

const apk = resolve(process.argv[2] || '');
const abi = process.argv[3] || (apk.includes('arm64') ? 'arm64-v8a' : apk.includes('x86_64') ? 'x86_64' : '');
if (!process.argv[2] || !['arm64-v8a', 'x86_64'].includes(abi)) {
  throw new Error('Usage: npm run android:verify -- <apk> [arm64-v8a|x86_64]');
}
const env = androidEnvironment();
const buildTools = join(env.ANDROID_HOME, 'build-tools', toolchain.buildTools);
run(join(env.JAVA_HOME, 'bin', windows ? 'java.exe' : 'java'),
  ['-jar', join(buildTools, 'lib/apksigner.jar'), 'verify', '--verbose', apk], env);
run(join(buildTools, windows ? 'zipalign.exe' : 'zipalign'), ['-c', '-P', '16', '4', apk], env);

const fd = openSync(apk, 'r');
const size = statSync(apk).size;
function bytes(offset, length) {
  if (!Number.isSafeInteger(offset) || !Number.isSafeInteger(length) || offset < 0 || length < 0 || offset + length > size) {
    throw new Error('APK ZIP offset is out of bounds');
  }
  const buffer = Buffer.alloc(length);
  if (readSync(fd, buffer, 0, length, offset) !== length) throw new Error('APK ZIP read is short');
  return buffer;
}
function elfDebugSize(entry) {
  if (entry.method !== 0) throw new Error('Native library must be stored for page alignment');
  const base = entry.dataOffset;
  const header = bytes(base, 64);
  if (header.subarray(0, 4).toString('hex') !== '7f454c46' || header[4] !== 2 || header[5] !== 1) {
    throw new Error('Native library is not little-endian ELF64');
  }
  const sectionOffset = Number(header.readBigUInt64LE(40));
  const sectionSize = header.readUInt16LE(58);
  const sectionCount = header.readUInt16LE(60);
  const namesIndex = header.readUInt16LE(62);
  if (!sectionSize || sectionCount > 4096 || namesIndex >= sectionCount || sectionOffset + sectionSize * sectionCount > entry.uncompressed) {
    throw new Error('Native ELF section table is invalid');
  }
  const sections = bytes(base + sectionOffset, sectionSize * sectionCount);
  const namesHeader = sections.subarray(namesIndex * sectionSize, (namesIndex + 1) * sectionSize);
  const namesOffset = Number(namesHeader.readBigUInt64LE(24));
  const namesLength = Number(namesHeader.readBigUInt64LE(32));
  if (namesOffset + namesLength > entry.uncompressed || namesLength > 1024 * 1024) {
    throw new Error('Native ELF names table is invalid');
  }
  const names = bytes(base + namesOffset, namesLength);
  let debug = 0;
  for (let index = 0; index < sectionCount; index++) {
    const section = sections.subarray(index * sectionSize, (index + 1) * sectionSize);
    const nameOffset = section.readUInt32LE(0);
    if (nameOffset >= names.length) continue;
    const end = names.indexOf(0, nameOffset);
    if (end < 0) continue;
    if (names.toString('utf8', nameOffset, end).startsWith('.debug_')) {
      debug += Number(section.readBigUInt64LE(32));
    }
  }
  return debug;
}

try {
  const tailLength = Math.min(size, 65557);
  const tail = bytes(size - tailLength, tailLength);
  let eocd = -1;
  for (let pos = tail.length - 22; pos >= 0; pos--) {
    if (tail.readUInt32LE(pos) === 0x06054b50 && pos + 22 + tail.readUInt16LE(pos + 20) === tail.length) {
      eocd = pos;
      break;
    }
  }
  if (eocd < 0) throw new Error('APK ZIP end directory is missing');
  const count = tail.readUInt16LE(eocd + 10);
  const directoryOffset = tail.readUInt32LE(eocd + 16);
  if (count === 0xffff || directoryOffset === 0xffffffff) throw new Error('ZIP64 APK is not supported by this verifier');
  const entries = [];
  let cursor = directoryOffset;
  for (let index = 0; index < count; index++) {
    const central = bytes(cursor, 46);
    if (central.readUInt32LE(0) !== 0x02014b50) throw new Error('APK central directory is invalid');
    const nameLength = central.readUInt16LE(28);
    const extraLength = central.readUInt16LE(30);
    const commentLength = central.readUInt16LE(32);
    const name = bytes(cursor + 46, nameLength).toString('utf8');
    const localOffset = central.readUInt32LE(42);
    const local = bytes(localOffset, 30);
    if (local.readUInt32LE(0) !== 0x04034b50) throw new Error('APK local header is invalid');
    const dataOffset = localOffset + 30 + local.readUInt16LE(26) + local.readUInt16LE(28);
    const compressed = central.readUInt32LE(20);
    entries.push({ name, localOffset, dataOffset, compressed, uncompressed: central.readUInt32LE(24), method: central.readUInt16LE(10) });
    cursor += 46 + nameLength + extraLength + commentLength;
  }
  const sorted = entries.toSorted((a, b) => a.localOffset - b.localOffset);
  let previousEnd = 0;
  let largestGap = 0;
  for (const entry of sorted) {
    if (entry.localOffset < previousEnd) throw new Error('APK local entries overlap');
    largestGap = Math.max(largestGap, entry.localOffset - previousEnd);
    previousEnd = entry.dataOffset + entry.compressed;
  }
  if (largestGap > 1024 * 1024) throw new Error(`APK contains ${largestGap} unreferenced bytes between entries`);
  const libraries = entries.filter(entry => entry.name.startsWith('lib/') && entry.name.endsWith('.so'));
  const abis = [...new Set(libraries.map(entry => entry.name.split('/')[1]))];
  if (abis.length !== 1 || abis[0] !== abi) throw new Error(`Expected only ${abi}, found ${abis.join(', ')}`);
  const native = libraries.find(entry => entry.name === `lib/${abi}/libmarklite_lib.so`);
  if (!native) throw new Error('MarkLite native library is missing');
  const debugBytes = elfDebugSize(native);
  if ((apk.includes('optimized') || process.argv.includes('--release')) && debugBytes) throw new Error(`Release APK retains ${debugBytes} ELF debug bytes`);
  console.log(JSON.stringify({ apk, bytes: size, entries: entries.length, abi, nativeBytes: native.uncompressed, debugBytes, largestGap }));
} finally {
  closeSync(fd);
}
