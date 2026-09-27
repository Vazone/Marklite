import { open } from 'node:fs/promises';

// Inspect the application executable, not the NSIS bootstrap (which can be 32-bit).
export async function verifyArchitecture(path, platform) {
  const file = await open(path, 'r');
  try {
    const read = async (offset, length) => {
      const buffer = Buffer.alloc(length);
      if ((await file.read(buffer, 0, length, offset)).bytesRead !== length) throw new Error('Truncated executable header');
      return buffer;
    };
    const header = await read(0, 64);
    let matches = false;
    if (platform === 'windows-x86_64' && header.toString('ascii', 0, 2) === 'MZ') {
      const pe = await read(header.readUInt32LE(60), 26);
      matches = pe.readUInt32LE(0) === 0x4550 && pe.readUInt16LE(4) === 0x8664 && pe.readUInt16LE(24) === 0x20b;
    } else if (platform === 'linux-x86_64') {
      matches = header.readUInt32BE(0) === 0x7f454c46 && header[4] === 2 && header[5] === 1 && header.readUInt16LE(18) === 62;
    } else if (platform === 'darwin-aarch64' || platform === 'darwin-x86_64') {
      const cpu = platform === 'darwin-aarch64' ? 0x100000c : 0x1000007;
      matches = header.readUInt32LE(0) === 0xfeedfacf && header.readUInt32LE(4) === cpu && header.readUInt32LE(12) === 2;
    }
    if (!matches) throw new Error(`Executable architecture mismatch: ${platform}`);
    return platform;
  } finally { await file.close(); }
}
