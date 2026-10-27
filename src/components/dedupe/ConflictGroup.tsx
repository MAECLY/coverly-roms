/**
 * A group whose copies carry different real saves (spec §8.3). Nothing in
 * it moves, so there is no shelf, no tray and no radio: the copies and
 * their saves are listed side by side (tags, format, size, dates and each
 * copy's own save; numbers in mono). Each copy and each save can be shown
 * in Finder / Explorer / its folder, so you can decide by hand.
 */
import { FolderSearch, TriangleAlert } from 'lucide-react';
import { useContext, useId, type KeyboardEvent } from 'react';

import { CopyFacts } from '@/components/dedupe/CopyFacts';
import { GroupHead, StateStamp } from '@/components/dedupe/DupGroup';
import { RevealButton } from '@/components/dedupe/RevealButton';
import { Button } from '@/components/ui/button';
import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import type { DupGroupView } from '@/ipc/dedupeTypes';
import { fileName, formatBytes } from '@/lib/dedupe';
import { formatDate } from '@/lib/format';
import { isRevealChord, isTyping } from '@/lib/keyboard';
import {
  revealCopyKey,
  revealLabelKey,
  revealOs,
  revealShortcut,
} from '@/lib/reveal';

import './dedupe.css';

export interface ConflictGroupProps {
  group: DupGroupView;
  /** Show a save in its folder (the OS file manager). */
  onReveal?: ((path: string) => void) | undefined;
  /** The system's display name, under the title with its icon. */
  systemName?: string | undefined;
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

export function ConflictGroup({
  group,
  onReveal,
  systemName,
}: ConflictGroupProps) {
  const t = useT();
  const locale = useContext(LocaleContext);
  const uid = useId();
  const titleId = `${uid}-title`;
  const saves = group.action.kind === 'conflict' ? group.action.saves : [];

  // ⌥⌘R / Shift+Alt+R shows the focused row's file (else the first copy),
  // as in a normal group; on macOS it must never reach WebKit's reload.
  function onKeyDown(e: KeyboardEvent<HTMLDivElement>) {
    if (isTyping(e.target) || !isRevealChord(e.nativeEvent, revealOs())) return;
    e.preventDefault();
    if (!onReveal) return;
    const row = (e.target as HTMLElement).closest<HTMLElement>(
      '[data-reveal-path]',
    );
    const path = row?.dataset.revealPath ?? group.members[0]?.unit.primary;
    if (path) onReveal(path);
  }

  return (
    // The group only delegates the reveal chord from its own buttons.
    // eslint-disable-next-line jsx-a11y/no-noninteractive-element-interactions
    <div
      role="group"
      aria-labelledby={titleId}
      onKeyDown={onKeyDown}
      className="dup-group dup-conflict"
      data-dup-group={group.id}
      data-conflict=""
      // No radio here: J/K and «Ver el conflicto» focus the group itself.
      tabIndex={-1}
      data-state="conflict"
    >
      <GroupHead
        titleId={titleId}
        title={group.title}
        system={group.system}
        systemName={systemName}
        serial={group.members.find((m) => m.serials[0])?.serials[0]}
        stamps={
          <StateStamp
            icon={TriangleAlert}
            text={t('dedupe.conflicts.badge')}
            tone="warning"
          />
        }
      />
      <p className="dup-conflict__note">{t('dedupe.conflicts.note')}</p>

      <div className="dup-conflict__list">
        {group.members.map((m) => (
          <div key={m.id} className="dup-row" data-reveal-path={m.unit.primary}>
            <span className="dup-row__pick" />
            <span className="dup-row__name">{fileName(m.unit.primary)}</span>
            <span className="dup-row__meta">
              {onReveal && (
                <RevealButton
                  path={m.unit.primary}
                  name={fileName(m.unit.primary)}
                  onReveal={onReveal}
                  shortcut
                />
              )}
            </span>
            <div className="dup-row__facts">
              <CopyFacts member={m} showNoSave />
            </div>
          </div>
        ))}
        {saves.map((s) => (
          <div
            key={s.path}
            className="dup-row dup-conflict__save"
            data-reveal-path={s.path}
          >
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
                  aria-label={t(revealCopyKey(), { name: fileName(s.path) })}
                  aria-keyshortcuts={revealShortcut().aria}
                  onClick={() => onReveal(s.path)}
                >
                  <FolderSearch aria-hidden />
                  {t(revealLabelKey())}
                </Button>
              </p>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
