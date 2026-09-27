import type { AppError } from './contracts';
import { isSameFilePath } from '../filePathIdentity';

/** References locate resources; they neither grant access nor establish file identity. */
export type ResourceRef =
  | { kind: 'desktopFile'; path: string }
  | { kind: 'desktopDirectory'; path: string }
  | { kind: 'androidDocument'; uri: string }
  | { kind: 'androidTree'; uri: string };

function invalid(): never {
  throw { code: 'INVALID_RESOURCE_REF', message: 'Invalid resource reference.' } satisfies AppError;
}

export function parseResourceRef(value: unknown): ResourceRef {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return invalid();
  const item = value as Record<string, unknown>;
  if (Object.keys(item).length !== 2) return invalid();
  if (item.kind === 'desktopFile' || item.kind === 'desktopDirectory') {
    if (typeof item.path !== 'string' || !item.path || item.path.includes('\0')) return invalid();
    // A native path is opaque here. The desktop adapter validates OS-specific absoluteness.
    return { kind: item.kind, path: item.path };
  }
  if (item.kind === 'androidDocument' || item.kind === 'androidTree') {
    if (typeof item.uri !== 'string' || !/^content:\/\/[^/\s?#]+(?:\/[^\s]*)?$/.test(item.uri)
      || /[\u0000-\u001f\u007f]/.test(item.uri)) return invalid();
    return { kind: item.kind, uri: item.uri };
  }
  return invalid();
}

export function desktopFile(path: string): ResourceRef {
  return parseResourceRef({ kind: 'desktopFile', path });
}

/** Correlates an async request with its original resource; it does not prove file identity. */
export function isSameResource(left: ResourceRef | null | undefined, right: ResourceRef | null | undefined): boolean {
  if (!left || !right || left.kind !== right.kind) return false;
  if (left.kind === 'desktopFile' || left.kind === 'desktopDirectory') {
    return isSameFilePath(left.path, (right as typeof left).path);
  }
  return left.uri === (right as typeof left).uri;
}

/** Reads legacy path-only documents while rejecting contradictory new protocol fields. */
export function documentResource(document: { path: string | null; resource?: ResourceRef | null }): ResourceRef | null {
  if (document.resource === undefined) return document.path === null ? null : desktopFile(document.path);
  const resource = document.resource === null ? null : parseResourceRef(document.resource);
  if (resource?.kind === 'desktopDirectory') return invalid();
  const projectedPath = resource?.kind === 'desktopFile' ? resource.path : null;
  if (projectedPath !== document.path) return invalid();
  return resource;
}

/** Compatibility boundary for the existing desktop path protocol. Never decodes a URI. */
export function desktopPath(resource: ResourceRef): string {
  const ref = parseResourceRef(resource);
  if (ref.kind !== 'desktopFile') {
    throw { code: 'RESOURCE_UNSUPPORTED', message: 'This adapter requires a desktop file.' } satisfies AppError;
  }
  return ref.path;
}
