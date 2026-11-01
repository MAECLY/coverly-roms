import { Folder } from 'lucide-react';
import { useContext } from 'react';

import { LocaleContext } from '@/i18n/core';
import { useRichT } from '@/i18n/rich';
import { useT } from '@/i18n/useT';
import type { OrganizeItem } from '@/ipc/types';
import { formatDecimalSize } from '@/lib/format';
import { titleIdKey } from '@/lib/organize';

import { hintText, pathText } from './styles';

/**
 * A row's name. A game folder (systems batch 2 §9.1) gets the folder glyph,
 * a visually hidden «Game folder» before its name, so the glyph carries no
 * information alone, and its size in mono right after it; `target` follows
 * as `→ path`. Any other item is its name (and target) as one path line.
 */
export function ItemName({
  item,
  target,
  className = '',
}: {
  item: OrganizeItem;
  /** Where it goes, shown after an arrow (moving rows only). */
  target?: string | null;
  className?: string;
}) {
  const t = useT();
  const locale = useContext(LocaleContext);
  const name = item.sources[0] ?? '';
  if (item.kind !== 'folder') {
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
      {target !== undefined && <span>{`→ ${target ?? ''}`}</span>}
    </span>
  );
}

/** Where the row's console title id was read, codes in mono (§10). */
export function TitleIdLine({ item }: { item: OrganizeItem }) {
  const tr = useRichT();
  const keyed = item.title_id ? titleIdKey(item.title_id) : null;
  if (!keyed) return null;
  return <p className={hintText}>{tr(keyed.key, keyed.vars)}</p>;
}
