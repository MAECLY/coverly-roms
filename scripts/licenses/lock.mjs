/**
 * Lock-file fingerprints for the generated third-party notices (spec D28).
 *
 * `LICENSES/generated/rust.txt` and `js.txt` start with
 * `lock-sha256: <hex>` of a normalised lock, so `pnpm notices:check` (pure
 * Node, no cargo-about) can tell when a dependency changed without the
 * notices being regenerated. Normalisation keeps the fingerprint stable
 * across what does not change the third-party set:
 * - CRLF → LF (Windows checkouts);
 * - in `Cargo.lock`, the workspace's own packages (those without a
 *   `source`) are dropped, since `bump-version.sh` rewrites their versions
 *   every release.
 */
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

/** The first line of every generated notices file. */
export const LOCK_PREFIX = 'lock-sha256: ';

const lf = (text) => text.replace(/\r\n?/g, '\n');

/** `Cargo.lock` with LF line ends and without the workspace's own crates. */
export function normaliseCargoLock(text) {
  const parts = lf(text).split(/^(?=\[\[package\]\]\n)/m);
  return parts
    .filter(
      (part) =>
        !part.startsWith('[[package]]\n') || /^source = /m.test(part),
    )
    .join('');
}

/** `pnpm-lock.yaml` with LF line ends. */
export function normalisePnpmLock(text) {
  return lf(text);
}

export const sha256 = (text) =>
  createHash('sha256').update(text, 'utf8').digest('hex');

/** The two generated files, each with the lock it fingerprints. */
export const NOTICE_FILES = [
  {
    kind: 'rust',
    file: 'LICENSES/generated/rust.txt',
    lock: 'Cargo.lock',
    normalise: normaliseCargoLock,
  },
  {
    kind: 'js',
    file: 'LICENSES/generated/js.txt',
    lock: 'pnpm-lock.yaml',
    normalise: normalisePnpmLock,
  },
];

/** The expected `lock-sha256` of one notices file under `root`. */
export function expectedHash(root, entry) {
  return sha256(entry.normalise(readFileSync(join(root, entry.lock), 'utf8')));
}

/** The hash a generated file declares on its first line, or null. */
export function declaredHash(text) {
  const first = lf(text).split('\n', 1)[0] ?? '';
  return first.startsWith(LOCK_PREFIX)
    ? first.slice(LOCK_PREFIX.length).trim()
    : null;
}

/**
 * Compares every generated file under `root` with its lock. Returns the
 * problems found (empty when up to date); never throws on a missing file.
 */
export function checkNotices(root) {
  const problems = [];
  for (const entry of NOTICE_FILES) {
    let text;
    try {
      text = readFileSync(join(root, entry.file), 'utf8');
    } catch {
      problems.push(`${entry.file} is missing`);
      continue;
    }
    const declared = declaredHash(text);
    const expected = expectedHash(root, entry);
    if (declared !== expected) {
      problems.push(`${entry.file} is out of date with ${entry.lock}`);
    }
  }
  return problems;
}
