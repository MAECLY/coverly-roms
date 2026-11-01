import {
  useContext,
  type CSSProperties,
  type JSX,
  type ReactNode,
} from 'react';

import { SystemIcon } from '@/components/system/SystemIcon';
import { useNoEmulator } from '@/hooks/useNoEmulator';
import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import { sectionLabel, type Section } from '@/lib/grouping';
import { cn } from '@/lib/utils';

/** Every view draws its section header rows this tall (spec §B.7). */
export const SECTION_ROW_H = 32;

export interface SectionHeaderRowProps {
  section: Section;
  /** Columns the header spans (`aria-colspan`). */
  colSpan: number;
  /** 0-based row index; `aria-rowindex` is 1-based. */
  rowIndex: number;
  style: CSSProperties;
  /** The header stuck to the top of the view. */
  sticky?: boolean;
  /** Drawn at the trailing end of the rule (Shelves' "See all"). */
  children?: ReactNode;
}

/** The shelf rule (brand §2): a group label with its count, on a 1px line
 *  with a 3px Buckram cap at the leading edge. A row holding one rowheader,
 *  never a rowheader row (spec §B.8). */
export function SectionHeaderRow({
  section,
  colSpan,
  rowIndex,
  style,
  sticky = false,
  children,
}: SectionHeaderRowProps): JSX.Element {
  const t = useT();
  const locale = useContext(LocaleContext);
  const group = sectionLabel(section, t);
  // Spoken and shown the same way ("1,204", "1.204").
  const count = section.count.toLocaleString(locale);
  // Once per shelf, after the name, as part of its name (systems batch 3
  // §9.1): a fact about the world, so no box and no warning colour.
  const noEmulator = useNoEmulator(section.systemId);
  const note = noEmulator ? t('systems.noEmulator') : null;
  return (
    <div
      role="row"
      aria-rowindex={rowIndex + 1}
      className={cn('section-row', sticky && 'section-row--sticky')}
      style={style}
    >
      <div
        role="rowheader"
        aria-colspan={colSpan}
        aria-label={t('layouts.groupCount', {
          group: note ? `${group} ${note}` : group,
          n: count,
        })}
        className="section-row__label"
      >
        {section.systemId !== undefined && (
          <SystemIcon systemId={section.systemId} size={20} />
        )}
        {/* The note rides in the name's ellipsis run: a narrow shelf cuts
            «· Sin emulador» first, never the name down to nothing. */}
        <span className="section-row__name">
          {group}
          {note && (
            <>
              {' '}
              <span aria-hidden className="section-row__note">
                ·
              </span>{' '}
              <span className="section-row__note">{note}</span>
            </>
          )}
        </span>
        <span className="section-row__count">{count}</span>
        {children}
      </div>
    </div>
  );
}
