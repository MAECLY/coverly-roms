/**
 * A group whose copies carry different real saves (spec §8.3). Nothing in
 * it moves, so there is no shelf, no tray and no radio: the copies and
 * their saves are listed side by side (size and modified date in mono),
 * each save with "Abrir carpeta", so you can decide by hand.
 */
import { FolderOpen, TriangleAlert } from 'lucide-react';
import { useContext, useId } from 'react';

import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import type { DupGroupView } from '@/ipc/dedupeTypes';
import { fileName, formatBytes } from '@/lib/dedupe';
import { formatDate } from '@/lib/format';

import './dedupe.css';

export interface ConflictGroupProps {
  group: DupGroupView;
  /** Show a save in its folder (the OS file manager). */
  onReveal?: ((path: string) => void) | undefined;
}

const MARK = '\u0000';

/** "Partida distinta: {path}" with only the path in mono. */
function SaveLabel({ text, path }: { text: string; path: string }) {
  const [before = '', after = ''] = text.split(MARK);
  return (
    <>
      <span className="dup-conflict__label">{before}</span>
      {path}
      <span className="dup-conflict__label">{after}</span>
    </>
  );
}

export function ConflictGroup({ group, onReveal }: ConflictGroupProps) {
  const t = useT();
  const locale = useContext(LocaleContext);
  const uid = useId();
  const titleId = `${uid}-title`;
  const saves = group.action.kind === 'conflict' ? group.action.saves : [];

  return (
    <div
      role="group"
      aria-labelledby={titleId}
      className="dup-group dup-conflict"
      data-dup-group={group.id}
      data-conflict=""
    >
      <div className="dup-group__head">
        <h3 id={titleId} className="dup-group__title">
          {group.title}
        </h3>
        <Badge variant="warning" className="dup-group__badge">
          <TriangleAlert size={14} aria-hidden />
          {t('dedupe.conflicts.badge')}
        </Badge>
      </div>
      <p className="dup-conflict__note">{t('dedupe.conflicts.note')}</p>

      <div className="dup-conflict__list">
        {group.members.map((m) => (
          <div key={m.id} className="dup-row">
            <span className="dup-row__pick" />
            <span className="dup-row__name">{fileName(m.unit.primary)}</span>
            <span className="dup-row__meta" />
            <span className="dup-row__size">{formatBytes(m.size, locale)}</span>
          </div>
        ))}
        {saves.map((s) => (
          <div key={s.path} className="dup-row dup-conflict__save">
            <span className="dup-row__pick" />
            <span className="dup-row__name">
              <SaveLabel
                text={t('dedupe.conflicts.row', { path: MARK })}
                path={s.path}
              />
            </span>
            <span className="dup-row__meta">
              <span className="dup-mono dup-conflict__date">
                {formatDate(Math.floor(s.mtime_ns / 1e9), locale)}
              </span>
            </span>
            <span className="dup-row__size">{formatBytes(s.size, locale)}</span>
            {onReveal && (
              <p className="dup-row__detail">
                <Button
                  variant="link"
                  size="sm"
                  className="dup-conflict__open"
                  aria-label={t('dedupe.conflicts.openFolderOf', {
                    path: s.path,
                  })}
                  onClick={() => onReveal(s.path)}
                >
                  <FolderOpen aria-hidden />
                  {t('dedupe.conflicts.openFolder')}
                </Button>
              </p>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
