/**
 * Reader of the generated third-party notices (`LICENSES/generated/*.txt`,
 * spec D28, §9.1 «Notices viewer»). The files can reach a few MB, so the
 * viewer never renders them as one `<pre>`: they are split here into one
 * block per licence text (the packages that share it, the licence, the
 * text) and searched in memory. The format is written by
 * `scripts/licenses/notices.mjs`.
 */

export type NoticesKind = 'rust' | 'js';

export interface NoticeBlock {
  /** `name version`, one per package that uses this text. */
  packages: string[];
  /** SPDX expression. */
  license: string;
  text: string;
  /** What the viewer shows and searches: the header lines, then the text. */
  body: string;
  /** Length of the header part of `body` (packages and licence). */
  headLength: number;
}

export interface Notices {
  blocks: NoticeBlock[];
  /** Distinct packages across every block. */
  packageCount: number;
}

export interface NoticeMatch {
  block: number;
  /** Offsets into the block's `body`. */
  start: number;
  end: number;
}

const RULE = '='.repeat(80);
const SUBRULE = '-'.repeat(80);
const PACKAGE = 'Package: ';
const LICENSE = 'License: ';

export function parseNotices(raw: string): Notices {
  const chunks = raw.replace(/\r\n?/g, '\n').split(`${RULE}\n`).slice(1);
  const blocks: NoticeBlock[] = [];
  const seen = new Set<string>();
  for (const chunk of chunks) {
    const lines = chunk.split('\n');
    const packages: string[] = [];
    let i = 0;
    for (; lines[i]?.startsWith(PACKAGE); i++) {
      packages.push(lines[i]!.slice(PACKAGE.length).trim());
    }
    const licenseLine = lines[i] ?? '';
    const license = licenseLine.startsWith(LICENSE)
      ? licenseLine.slice(LICENSE.length).trim()
      : '';
    const textStart = lines[i + 1] === SUBRULE ? i + 2 : i + 1;
    const text = lines.slice(textStart).join('\n').trim();
    if (packages.length === 0) continue;
    for (const p of packages) seen.add(p);
    const head = `${packages.join('\n')}\n${license}`;
    blocks.push({
      packages,
      license,
      text,
      body: `${head}\n\n${text}`,
      headLength: head.length,
    });
  }
  return { blocks, packageCount: seen.size };
}

/** Every case-insensitive occurrence of `query`, block by block. */
export function findMatches(
  blocks: readonly NoticeBlock[],
  query: string,
): NoticeMatch[] {
  const needle = query.trim().toLowerCase();
  if (!needle) return [];
  const out: NoticeMatch[] = [];
  blocks.forEach((b, block) => {
    const hay = b.body.toLowerCase();
    for (
      let at = hay.indexOf(needle);
      at >= 0;
      at = hay.indexOf(needle, at + needle.length)
    ) {
      out.push({ block, start: at, end: at + needle.length });
    }
  });
  return out;
}

/** Loads one generated file on demand (a lazy chunk; works in the mock). */
export async function loadNotices(kind: NoticesKind): Promise<Notices> {
  const mod =
    kind === 'rust'
      ? await import('../../LICENSES/generated/rust.txt?raw')
      : await import('../../LICENSES/generated/js.txt?raw');
  return parseNotices(mod.default);
}
