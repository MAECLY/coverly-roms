/**
 * Which copy explains a keychain error (spec D16, §7.3). Each OS gets its
 * own fix where it has one (`errors.keychainOs.<kind>.<os>`, e.g. the
 * macOS `security unlock-keychain` hint); any other platform, and a kind an
 * OS does not have (Windows has no «locked»), keeps the generic
 * `errors.keychain-<kind>` line.
 */
import type { KeychainKind } from '@/ipc/types';

/** `errors.keychainOs.<kind>.<os>` when the catalogue has it, else
 * `errors.keychain-<kind>`. `has` says whether a key resolves. */
export function keychainMessageKey(
  kind: KeychainKind,
  platform: string,
  has: (key: string) => boolean,
): string {
  const fallback = `errors.keychain-${kind}`;
  // A platform is a bare word (`std::env::consts::OS`); anything else would
  // walk a different branch of the catalogue.
  if (!/^[a-z0-9_]+$/.test(platform)) return fallback;
  const specific = `errors.keychainOs.${kind}.${platform}`;
  return has(specific) ? specific : fallback;
}
