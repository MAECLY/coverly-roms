import { Badge } from '@/components/ui/badge';
import { useT, type TFunction } from '@/i18n/useT';
import type { CandidateView, PickReason, SourceId } from '@/ipc/types';
import { KNOWN_SOURCES } from '@/lib/settingsRules';

type TFn = TFunction;

/** Reasons the UI has words for; anything else (an older or newer backend,
 * hostile data) is not explained rather than shown as a raw key. */
const SIMPLE_REASONS = [
  'only',
  'better-match',
  'release',
  'embedded-first',
  'language',
  'tie',
  'user',
] as const;

const isKnownSource = (id: string): id is SourceId =>
  (KNOWN_SOURCES as readonly string[]).includes(id);

/** The source's display name, or its raw id when this build does not know it. */
function sourceName(t: TFn, id: string): string {
  return isKnownSource(id) ? t(`settings.sources.name.${id}`) : id;
}

const count = (v: unknown): number =>
  typeof v === 'number' && Number.isFinite(v) ? v : 0;

/** The localised "why this image" sentence; empty for a reason it cannot name. */
// eslint-disable-next-line react-refresh/only-export-components -- shared with the drawer and queue (plan interface)
export function reasonText(t: TFn, r: PickReason): string {
  switch (r.why) {
    case 'votes':
      return t('reason.votes', { score: count(r.score), next: count(r.next) });
    case 'consensus':
      return t('reason.consensus', {
        count: count(r.count),
        next: count(r.next),
      });
    case 'source-order':
      return t('reason.source-order', {
        source: sourceName(t, String(r.source)),
      });
    case 'region':
      return t(r.rom ? 'reason.region-rom' : 'reason.region');
    default:
      return (SIMPLE_REASONS as readonly string[]).includes(r.why)
        ? t(`reason.${r.why}`)
        : '';
  }
}

/** Source badge, the key/regions/score line, votes and the reason of a pick.
 * The source is named once, on the Manila tag, never again in the line.
 * `compact` (narrow lists with no number keys) leaves the key out. */
export function CandidateMeta({
  c,
  position,
  compact = false,
}: {
  c: CandidateView;
  position: number;
  compact?: boolean;
}) {
  const t = useT();
  const source = sourceName(t, c.source);
  const why = c.reason ? reasonText(t, c.reason) : '';
  return (
    <>
      <Badge
        variant="tag"
        mono
        className="block max-w-full justify-self-start truncate leading-5"
        data-source={isKnownSource(c.source) ? c.source : 'other'}
      >
        {source}
      </Badge>
      <span className="hint">
        {t(compact ? 'review.metaShort' : 'review.meta', {
          key: position,
          regions: c.regions.map((r) => t(`regions.${r}`)).join(', ') || '—',
          score: Math.round(c.confidence * 100),
        })}
      </span>
      {c.votes && (
        <span className="hint">
          {t('review.votes', {
            score: count(c.votes.up) - count(c.votes.down),
            up: count(c.votes.up),
            down: count(c.votes.down),
          })}
        </span>
      )}
      {why && <span className="hint">{t('review.why', { reason: why })}</span>}
    </>
  );
}
