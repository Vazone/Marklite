export function windowsPathIdentity(path: string | null | undefined): string | null {
  if (!path || /[\u0000-\u001f]/.test(path)) return null;

  let normalized = path.trim().replaceAll('/', '\\');
  if (!normalized) return null;
  if (/^\\\\\.\\/.test(normalized)) return null;
  if (/^\\\\\?\\UNC\\/i.test(normalized)) {
    normalized = `\\\\${normalized.slice(8)}`;
  } else if (/^\\\\\?\\/.test(normalized)) {
    normalized = normalized.slice(4);
  }

  let root: string;
  let segments: string[];
  let driveRoot = false;
  const drive = normalized.match(/^([a-z]):\\/i);
  if (drive) {
    root = `${drive[1].toLowerCase()}:\\`;
    segments = normalized.slice(3).split(/\\+/);
    driveRoot = true;
  } else if (normalized.startsWith('\\\\')) {
    const uncParts = normalized.slice(2).split(/\\+/).filter(Boolean);
    if (uncParts.length < 2) return null;
    root = `\\\\${uncParts[0]}\\${uncParts[1]}`;
    segments = uncParts.slice(2);
  } else {
    return null;
  }

  const stack: string[] = [];
  for (const segment of segments) {
    if (!segment || segment === '.') continue;
    if (segment === '..') {
      if (!stack.length) return null;
      stack.pop();
      continue;
    }
    stack.push(segment);
  }

  const suffix = stack.join('\\');
  const identity = driveRoot ? `${root}${suffix}` : `${root}${suffix ? `\\${suffix}` : ''}`;
  return identity.toLowerCase();
}

export function isSameWindowsFilePath(
  left: string | null | undefined,
  right: string | null | undefined
): boolean {
  const leftIdentity = windowsPathIdentity(left);
  const rightIdentity = windowsPathIdentity(right);
  return leftIdentity !== null && rightIdentity !== null && leftIdentity === rightIdentity;
}
