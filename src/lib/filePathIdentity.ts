/**
 * File identities are opaque values issued by the desktop filesystem boundary.
 * Frontend code may compare them for exact equality, but must never normalize,
 * parse, derive, or display them as paths.
 */
export type FileIdentity = string;

export function isSameFileIdentity(
  left: FileIdentity | null | undefined,
  right: FileIdentity | null | undefined
): boolean {
  return typeof left === 'string' && left.length > 0 && left === right;
}

/**
 * Exact path equality is only for correlating an async request with the tab
 * that issued it and for legacy session entries which have not been loaded.
 * It deliberately makes no claim about filesystem identity.
 */
export function isSameFilePath(
  left: string | null | undefined,
  right: string | null | undefined
): boolean {
  return typeof left === 'string' && left.length > 0 && left === right;
}
