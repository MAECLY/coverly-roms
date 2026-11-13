/**
 * Version strings for DISPLAY ORDER ONLY.
 *
 * «Newest» is never decided here: Rust compares versions with
 * `dedupe::keeper::version_order` (hex on `xbox360`) and sends the result
 * as `installed` and `newer_elsewhere`, with each game's children already
 * ordered (addon views D5). This module only sorts the Versión column and
 * writes a version with its `v`.
 */

const COLLATOR = new Intl.Collator('en', {
  numeric: true,
  sensitivity: 'base',
});

function bare(v: string): string {
  return /^[vV]/.test(v) ? v.slice(1) : v;
}

/**
 * Natural text order (`1.0.2` < `1.0.10`), a leading `v` ignored, a missing
 * version last. For sorting what is shown, never for «la más nueva».
 */
export function compareVersions(
  a: string | undefined,
  b: string | undefined,
): number {
  if (a === undefined || b === undefined)
    return a === b ? 0 : a === undefined ? 1 : -1;
  return COLLATOR.compare(bare(a), bare(b));
}

/** `1.05` → `v1.05`; a version already written with its `v` is kept. */
export function displayVersion(v: string): string {
  return /^[vV]/.test(v) ? v : `v${v}`;
}
