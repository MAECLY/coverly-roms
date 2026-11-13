/**
 * What a game carries, as short visible parts and as one spoken phrase
 * (addon views §8.2, §8.9).
 *
 * The visible parts (`+3 DLC`, `2 rev.`, `v1.05` or `↑ falta v1.3.0`) go
 * under the cover, never over the art, and are `aria-hidden`; the spoken
 * phrase says the whole state in words, so nothing relies on colour, an
 * arrow or a dot. «Newest» and «missing» come from Rust (`installed`,
 * `newer_elsewhere`); nothing here compares versions.
 */
import type { TFunction } from '@/i18n/useT';

import type { Game } from './games';
import { displayVersion } from './versions';

export type SummaryPart =
  | { kind: 'dlc'; n: number }
  | { kind: 'rev'; n: number }
  | { kind: 'version'; v: string }
  | { kind: 'missing'; v: string };

/**
 * The visible summary, in order: DLC count, revision count, then the
 * installed version or, when a newer update is elsewhere, the warning.
 * Demos and orphans have none (a demo shows «Demo de …» instead).
 */
export function summaryParts(game: Game): SummaryPart[] {
  if (game.isDemo || game.orphan !== null) return [];
  const parts: SummaryPart[] = [];
  if (game.dlc.length > 0) parts.push({ kind: 'dlc', n: game.dlc.length });
  if (game.revisions.length > 1)
    parts.push({ kind: 'rev', n: game.revisions.length });
  if (game.missing) parts.push({ kind: 'missing', v: game.missing.version });
  else if (game.installed !== null)
    parts.push({ kind: 'version', v: game.installed });
  return parts;
}

/** One visible part in the approved copy: `+3 DLC`, `2 rev.`, `v1.05`,
 * `↑ falta v1.3.0`. */
export function summaryText(part: SummaryPart, t: TFunction): string {
  switch (part.kind) {
    case 'dlc':
      return t('content.summary.dlc', { n: part.n });
    case 'rev':
      return t('content.summary.rev', { n: part.n });
    case 'version':
      return displayVersion(part.v);
    case 'missing':
      return t('content.summary.missing', { v: part.v.replace(/^[vV]/, '') });
  }
}

/** The ` · ` between two parts, in characters. */
const SEPARATOR_CHARS = 3;

/**
 * The parts that fit in `maxChars` monospaced characters: `+N DLC` goes
 * first, then `N rev.`; the version, and above all the warning, always
 * stay (§8.2). The whole state is in the tile's title and spoken phrase.
 */
export function fitSummary(
  parts: readonly SummaryPart[],
  t: TFunction,
  maxChars: number,
): SummaryPart[] {
  const kept = [...parts];
  const width = () =>
    kept.reduce((w, p) => w + summaryText(p, t).length, 0) +
    SEPARATOR_CHARS * Math.max(0, kept.length - 1);
  for (const drop of ['dlc', 'rev'] as const) {
    if (width() <= maxChars) break;
    const i = kept.findIndex((p) => p.kind === drop);
    if (i >= 0) kept.splice(i, 1);
  }
  return kept;
}

/**
 * Whether any game shows add-on content (updates, DLC, more revisions, a
 * newer update elsewhere, or is a demo): the library flag that gives cover tiles their summary line.
 */
export function hasContent(games: readonly Game[]): boolean {
  return games.some(
    (g) =>
      g.children.length > 0 ||
      g.revisions.length > 1 ||
      g.isDemo ||
      g.missing !== null,
  );
}

export interface SpokenOptions {
  /** A child of this game is tagged «Nuevo» (D18). */
  isNew?: boolean;
  /** For `Intl.NumberFormat`; the runtime's default when absent. */
  locale?: string;
}

interface Memo {
  t: TFunction;
  key: string;
  text: string;
}

/** Built once per game and arguments (§8.9: screen-reader density). */
const memo = new WeakMap<Game, Memo>();

function build(
  game: Game,
  systemName: string,
  t: TFunction,
  demoOfName: string | undefined,
  opts: SpokenOptions,
): string {
  const n = new Intl.NumberFormat(opts.locale);
  const count = (base: string, value: number) =>
    value === 1 ? t(`${base}One`) : t(base, { n: n.format(value) });
  const parts: string[] = [];
  if (game.isDemo) {
    const full = demoOfName ?? game.entry.demo?.of_name;
    parts.push(
      game.demoOf !== null && full !== undefined
        ? t('spoken.demoOf', { name: full })
        : t('spoken.demoAlone'),
    );
  }
  if (game.dlc.length > 0)
    parts.push(t('spoken.dlc', { n: n.format(game.dlc.length) }));
  if (game.updates.length > 0) {
    parts.push(count('spoken.updates', game.updates.length));
    if (game.installed !== null)
      parts.push(t('spoken.newest', { v: displayVersion(game.installed) }));
  } else if (game.installed !== null && !game.missing && !game.isDemo) {
    // No update in the library: the chip still shows the base's own
    // version (`summaryParts`), so the phrase says it too.
    parts.push(displayVersion(game.installed));
  }
  if (game.missing)
    parts.push(
      t('spoken.missing', { v: displayVersion(game.missing.version) }),
    );
  if (game.revisions.length > 1) {
    parts.push(t('spoken.revisions', { n: n.format(game.revisions.length) }));
    if (game.entry.revision !== undefined)
      parts.push(t('spoken.revisionUsed', { rev: game.entry.revision }));
  }
  if (game.demos.length > 0) parts.push(t('spoken.hasDemo'));
  if (opts.isNew) parts.push(t('spoken.newContent'));
  const head = t('spoken.head', { title: game.entry.name, system: systemName });
  return parts.length === 0
    ? head
    : t('spoken.full', { head, parts: parts.join(', ') });
}

/**
 * The accessible name of a game's tile, box or row: «Tidewalker, Switch.
 * 1 DLC, 1 actualización, la más nueva v1.0.2, falta la última
 * actualización, v1.3.0». The visible name is always its start (WCAG
 * 2.5.3). `demoOfName` overrides the full game's name the scan sent.
 */
export function spokenSummary(
  game: Game,
  systemName: string,
  t: TFunction,
  demoOfName?: string,
  opts: SpokenOptions = {},
): string {
  const key = JSON.stringify([
    systemName,
    demoOfName ?? null,
    opts.isNew === true,
    opts.locale ?? null,
  ]);
  const hit = memo.get(game);
  if (hit && hit.t === t && hit.key === key) return hit.text;
  const text = build(game, systemName, t, demoOfName, opts);
  memo.set(game, { t, key, text });
  return text;
}
