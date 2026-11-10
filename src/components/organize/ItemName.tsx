import { Folder, Lock } from 'lucide-react';
import { useContext, useId, type ReactNode } from 'react';

import { Badge } from '@/components/ui/badge';
import { LocaleContext } from '@/i18n/core';
import { useRichT } from '@/i18n/rich';
import { useT } from '@/i18n/useT';
import type { OrganizeItem } from '@/ipc/types';
import { formatDecimalSize } from '@/lib/format';
import {
  addonText,
  addonViaKey,
  contentIdKey,
  encryptedName,
  titleIdKey,
  titleSourceKey,
} from '@/lib/organize';

import { hintText, pathText } from './styles';

/**
 * A row's name. A game folder (systems batch 2 §9.1) gets the folder glyph,
 * a visually hidden «Game folder» before its name, so the glyph carries no
 * information alone, and its size in mono right after it; `target` follows
 * as `→ path`. Any other item is its name (and target) as one path line.
 *
 * A Switch or 3DS title nothing could read without decrypting it (spec
 * D11) carries the «Cifrado» label after its name, described by a visible
 * line under it that says why; the label is a word, never a glyph alone,
 * and the row says «Cifrado» only there.
 */
export function ItemName({
  item,
  target,
  fullTarget,
  className = '',
}: {
  item: OrganizeItem;
  /** Where it goes, shown after an arrow (moving rows only). */
  target?: string | null;
  /** The whole target when `target` is shortened (`…/Updates/<name>`):
   * its tooltip and what a screen reader reads. */
  fullTarget?: string | null;
  className?: string;
}) {
  const t = useT();
  const detail = useId();
  if (!encryptedName(item))
    return (
      <Name
        item={item}
        target={target}
        fullTarget={fullTarget}
        className={className}
      />
    );
  return (
    <span className={`grid min-w-0 gap-0.5 ${className}`}>
      <span className="inline-flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
        <Name
          item={item}
          target={target}
          fullTarget={fullTarget}
          className="min-w-0"
        />
        <Badge variant="outline" aria-describedby={detail}>
          <Lock aria-hidden />
          {t('organize.encryptedLabel')}
        </Badge>
      </span>
      <span id={detail} className={hintText}>
        {t('organize.encryptedUnresolved')}
      </span>
    </span>
  );
}

/** `→ target`; a shortened target keeps the whole path as its tooltip
 * and as the text a screen reader reads. */
function Arrow({
  target,
  full,
}: {
  target: string | null;
  full: string | null | undefined;
}): ReactNode {
  if (!full) return `→ ${target ?? ''}`;
  return (
    <>
      <span aria-hidden title={full}>{`→ ${target ?? ''}`}</span>
      <span className="sr-only">{`→ ${full}`}</span>
    </>
  );
}

function Name({
  item,
  target,
  fullTarget,
  className,
}: {
  item: OrganizeItem;
  target?: string | null | undefined;
  fullTarget?: string | null | undefined;
  className: string;
}): ReactNode {
  const t = useT();
  const locale = useContext(LocaleContext);
  const name = item.sources[0] ?? '';
  if (item.kind !== 'folder') {
    if (target !== undefined && fullTarget)
      return (
        <span className={`${pathText} ${className}`}>
          {`${name} `}
          <Arrow target={target} full={fullTarget} />
        </span>
      );
    const text = target === undefined ? name : `${name} → ${target ?? ''}`;
    return <span className={`${pathText} ${className}`}>{text}</span>;
  }
  return (
    <span
      className={`${pathText} inline-flex flex-wrap items-baseline gap-x-2 ${className}`}
    >
      <Folder
        aria-hidden
        className="size-3.5 shrink-0 self-center text-cr-text-secondary"
      />
      <span>
        <span className="sr-only">{`${t('organize.item.folder')} `}</span>
        {name}
      </span>
      <span className="whitespace-nowrap font-mono tabular-nums text-cr-text-secondary">
        {formatDecimalSize(item.size, locale)}
      </span>
      {target !== undefined && (
        <span>
          <Arrow target={target} full={fullTarget} />
        </span>
      )}
    </span>
  );
}

/** Where the row's catalogue name came from, where its console title id
 * was read, then its content id, each its own line with the codes in mono
 * (§10). */
export function TitleIdLine({ item }: { item: OrganizeItem }) {
  const tr = useRichT();
  const source = titleSourceKey(item);
  const keyed = item.title_id ? titleIdKey(item.title_id) : null;
  const content = contentIdKey(item);
  if (!source && !keyed && !content) return null;
  return (
    <>
      {/* A game name, not a code: in the body face. */}
      {source && <p className={hintText}>{tr(source.key, source.vars, [])}</p>}
      {keyed && <p className={hintText}>{tr(keyed.key, keyed.vars)}</p>}
      {content && <p className={hintText}>{tr(content.key, content.vars)}</p>}
    </>
  );
}

/**
 * What a mod, patch or cheats row is and why Coverly thinks it belongs to
 * its game (mods spec §9.1): the kind in words, its format and key in
 * mono, then the link proof as a plain sentence. Never a badge, never a
 * colour: both lines are text.
 */
export function AddonLine({
  item,
  owner = null,
}: {
  item: OrganizeItem;
  /** Whose it is («Mod de Pokemon Y»), read first. */
  owner?: string | null;
}) {
  const t = useT();
  const tr = useRichT();
  const link = item.game?.addon;
  if (!link) return null;
  const line = addonText(link);
  const what = t(String(line.vars.what));
  const via = addonViaKey(link);
  return (
    <>
      {owner && <span className="text-caption text-cr-text">{owner}</span>}
      <span className="text-caption text-cr-text-secondary [overflow-wrap:anywhere]">
        {tr(line.key, { ...line.vars, what }, ['format', 'key'])}
      </span>
      <span className="text-caption text-cr-text-secondary [overflow-wrap:anywhere]">
        {tr(via.key, via.vars, ['key'])}
      </span>
    </>
  );
}
