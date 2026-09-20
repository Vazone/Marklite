import { existsSync, readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';

export function createRepositoryIndex(repositoryRoot) {
  const root = path.resolve(repositoryRoot);
  const fileCache = new Map();
  const walkCache = new Map();
  const physicalReads = new Map();

  function normalize(relativePath) {
    return relativePath.split(path.sep).join('/');
  }

  function absolute(relativePath) {
    return path.join(root, relativePath);
  }

  function exists(relativePath) {
    return existsSync(absolute(relativePath));
  }

  function read(relativePath) {
    const normalized = normalize(relativePath);
    if (!fileCache.has(normalized)) {
      fileCache.set(normalized, readFileSync(absolute(normalized), 'utf8'));
      physicalReads.set(normalized, (physicalReads.get(normalized) ?? 0) + 1);
    }
    return fileCache.get(normalized);
  }

  function readJson(relativePath) {
    return JSON.parse(read(relativePath));
  }

  function walk(relativeDirectory) {
    const normalizedDirectory = normalize(relativeDirectory);
    if (walkCache.has(normalizedDirectory)) {
      return [...walkCache.get(normalizedDirectory)];
    }
    if (!exists(normalizedDirectory)) {
      walkCache.set(normalizedDirectory, []);
      return [];
    }

    const files = readdirSync(absolute(normalizedDirectory), { withFileTypes: true }).flatMap(
      (entry) => {
        const child = normalize(path.join(normalizedDirectory, entry.name));
        return entry.isDirectory() ? walk(child) : [child];
      }
    );
    walkCache.set(normalizedDirectory, files);
    return [...files];
  }

  return {
    root,
    absolute,
    exists,
    read,
    readJson,
    walk,
    physicalReadCount(relativePath) {
      return physicalReads.get(normalize(relativePath)) ?? 0;
    }
  };
}

export function createCheckCollector() {
  const errors = [];
  let checks = 0;

  return {
    check(condition, message) {
      checks += 1;
      if (!condition) errors.push(message);
    },
    snapshot() {
      return { errors: [...errors], checks };
    }
  };
}

export function headings(content, level = 2) {
  const expression = new RegExp(`^${'#'.repeat(level)}\\s+(.+?)\\s*$`, 'gm');
  return new Set([...content.matchAll(expression)].map((match) => match[1]));
}

export function markdownTableRows(content) {
  return content
    .split(/\r?\n/)
    .filter((line) => /^\|.*\|\s*$/.test(line))
    .map((line) =>
      line
        .slice(1, line.lastIndexOf('|'))
        .split('|')
        .map((cell) => cell.trim())
    )
    .filter((cells) => /^\d{4}$/.test(cells[0] ?? ''));
}

export function withoutCodeTicks(value) {
  return value.replace(/^`|`$/g, '');
}
