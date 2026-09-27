import type { AppError } from './contracts';
import { parseResourceRef, type ResourceRef } from './resources';

export type RecoverySnapshot = {
  id: string;
  revision: number;
  resource: ResourceRef | null;
  title: string;
  content: string;
  baseFileIdentity: string | null;
  baseContentVersion: string | null;
};
export type RecoveryReceipt = { id: string; revision: number; checksum: string; updatedAt: string };
export type RecoveryEntry = {
  receipt: RecoveryReceipt; title: string; resource: ResourceRef | null; contentBytes: number; stale: boolean;
};
export type RecoveryInventory = { entries: RecoveryEntry[]; issues: { id: string; error: AppError }[] };

function invalid(): never {
  throw { code: 'INVALID_RESPONSE', message: 'Invalid recovery response.' } satisfies AppError;
}
function object(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return invalid();
  return value as Record<string, unknown>;
}
function string(value: unknown): string { return typeof value === 'string' ? value : invalid(); }
function optional(value: unknown): string | null { return value === null ? null : string(value); }
function resource(value: unknown): ResourceRef | null {
  if (value === null) return null;
  const result = parseResourceRef(value);
  return result.kind === 'desktopDirectory' ? invalid() : result;
}
function id(value: unknown): string {
  const result = string(value);
  return /^[a-zA-Z0-9-]{1,80}$/.test(result) ? result : invalid();
}
function integer(value: unknown): number {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= 0 ? value : invalid();
}

export function parseRecoveryReceipt(value: unknown): RecoveryReceipt {
  const item = object(value);
  const checksum = string(item.checksum), updatedAt = string(item.updatedAt);
  if (!/^sha256:[a-f0-9]{64}$/.test(checksum) || !Number.isFinite(Date.parse(updatedAt))) return invalid();
  return { id: id(item.id), revision: integer(item.revision), checksum, updatedAt };
}

export function parseRecoverySnapshot(value: unknown): RecoverySnapshot {
  const item = object(value);
  return { id: id(item.id), revision: integer(item.revision),
    resource: resource(item.resource),
    title: string(item.title), content: string(item.content),
    baseFileIdentity: optional(item.baseFileIdentity), baseContentVersion: optional(item.baseContentVersion) };
}

export function parseRecoveryInventory(value: unknown): RecoveryInventory {
  const item = object(value);
  if (!Array.isArray(item.entries) || !Array.isArray(item.issues)) return invalid();
  return {
    entries: item.entries.map(value => {
      const entry = object(value);
      if (typeof entry.stale !== 'boolean') return invalid();
      return { receipt: parseRecoveryReceipt(entry.receipt), title: string(entry.title),
        resource: resource(entry.resource),
        contentBytes: integer(entry.contentBytes), stale: entry.stale };
    }),
    issues: item.issues.map(value => {
      const issue = object(value), error = object(issue.error);
      return { id: string(issue.id), error: { code: string(error.code), message: string(error.message) } };
    })
  };
}
