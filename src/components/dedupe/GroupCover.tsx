/**
 * A duplicate group's cover (brand §2 slot, 3:4) and a copy's own thumbnail.
 *
 * The cover is the keeper's art, else another copy's, else an image the
 * fill already downloaded from libretro-thumbnails (Rust decides, D4). The
 * slot draws its outline in while the bytes load, fades the image in, and
 * with no art at all stays the "Missing" slot: dashed outline, four
 * photo-corners and «Sin portada». A missing cover is never an error.
 *
 * Only rows the group window mounted ask (`enabled`), and Organise passes
 * no plan, so it never renders these.
 */
import { ImageOff } from 'lucide-react';

import { useDedupeArt } from '@/hooks/useDedupeArt';
import { useT } from '@/i18n/useT';
import type { DupGroupView, MediaView, MemberView } from '@/ipc/dedupeTypes';
import { coverArt, coverMedia, ownArtDiffers } from '@/lib/dedupe';
import { cn } from '@/lib/utils';

import './dedupe.css';

/** The slot in viewBox units; CSS scales it to 72×96 or 48×64. */
const W = 72;
const H = 96;
/** Photo-corner leg (brand: only on slots of 24 px and larger). */
const CORNER = 11;

function Outline({ state }: { state: 'loading' | 'ready' | 'missing' }) {
  return (
    <svg
      className="dup-cover__outline"
      viewBox={`0 0 ${W} ${H}`}
      preserveAspectRatio="none"
      aria-hidden
    >
      <rect
        x="0.75"
        y="0.75"
        width={W - 1.5}
        height={H - 1.5}
        rx="3"
        fill="none"
        vectorEffect="non-scaling-stroke"
        // Loading draws the outline in once around (dash = whole length);
        // missing is the brand dash, 4 on, 3 off.
        {...(state === 'loading' ? { pathLength: 100 } : {})}
      />
      {state === 'missing' && (
        <g className="dup-cover__corners">
          <path d={`M0 0H${CORNER}L0 ${CORNER}Z`} />
          <path d={`M${W} 0V${CORNER}L${W - CORNER} 0Z`} />
          <path d={`M0 ${H}V${H - CORNER}L${CORNER} ${H}Z`} />
          <path d={`M${W} ${H}H${W - CORNER}L${W} ${H - CORNER}Z`} />
        </g>
      )}
    </svg>
  );
}

export interface GroupCoverProps {
  planId: number;
  group: DupGroupView;
  /** The group row is mounted (windowed lists mount only nearby rows). */
  enabled: boolean;
  /** `compact` is always 48×64; `head` is 72×96, 48×64 on narrow windows. */
  size?: 'head' | 'compact';
  /** A game left alone dims its cover (spec §7.1, skipped). */
  dim?: boolean;
}

export function GroupCover({
  planId,
  group,
  enabled,
  size = 'head',
  dim = false,
}: GroupCoverProps) {
  const t = useT();
  const { media, caption } = coverMedia(group);
  const art = useDedupeArt(
    planId,
    group.id,
    null,
    media,
    enabled,
    group.keeper,
  );
  // No copy has art in that media, yet bytes came: the libretro cache.
  const fromCache = art.status === 'ready' && coverArt(group, media) === null;

  return (
    <div
      className={cn('dup-cover', dim && 'dup-dim')}
      data-size={size}
      data-state={art.status}
    >
      <div
        className="dup-cover__slot"
        title={fromCache ? t('dedupe.cover.fromCache') : undefined}
      >
        <Outline state={art.status} />
        {art.status === 'ready' && (
          <img
            key={art.url}
            className="dup-cover__img"
            src={art.url}
            alt=""
            decoding="async"
            draggable={false}
          />
        )}
        {art.status === 'missing' && (
          <span className="dup-cover__missing">
            <ImageOff size={18} aria-hidden />
            <span>{t('dedupe.cover.missing')}</span>
          </span>
        )}
      </div>
      {caption && <p className="dup-cover__caption">{t(caption)}</p>}
    </div>
  );
}

export interface OwnArtThumbProps {
  planId: number;
  group: DupGroupView;
  member: MemberView;
  /** The cover's media: a copy's art in another media is not compared. */
  media: MediaView;
  enabled: boolean;
  /** A game left alone dims the copy's image. */
  dim?: boolean;
}

/**
 * A copy's own image, 32 px wide, only when it differs from the group cover
 * by path and size (D6, D7): a Japanese box beside a World one is exactly
 * what helps decide. Nothing renders for the copy the cover came from.
 */
export function OwnArtThumb({
  planId,
  group,
  member,
  media,
  enabled,
  dim = false,
}: OwnArtThumbProps) {
  const t = useT();
  const differs = ownArtDiffers(member, coverArt(group, media), media);
  const art = useDedupeArt(
    planId,
    group.id,
    member.id,
    media,
    enabled && differs,
  );
  if (!differs || art.status === 'missing') return null;
  const label = t('dedupe.cover.ownArt');
  return (
    <span
      className={cn('dup-own-art', dim && 'dup-dim')}
      data-state={art.status}
      title={label}
    >
      {art.status === 'ready' && (
        <img src={art.url} alt={label} decoding="async" draggable={false} />
      )}
    </span>
  );
}
