import { useVirtualizer } from '@tanstack/react-virtual';
import { ChevronRight, Plus } from 'lucide-react';
import { useId, useRef } from 'react';

import { Tip } from '@/components/library/Tip';
import { useT } from '@/i18n/useT';
import type { CategoryRef, ExtraFileView } from '@/ipc/types';
import { categoryLabel, diskName } from '@/lib/extras';

import { ExtraRow } from './ExtraRow';

/** More files than this in one category: the band starts collapsed and
 * renders only the rows in view (extras spec §8.2). */
export const VIRTUALISE_AFTER = 200;
/** A one-line row's height, in px (the measured height wins). */
const ROW_H = 34;
/** The inner list's height the virtualiser starts from (22rem). */
const LIST_H = 352;

export interface ExtrasCategoryProps {
  category: CategoryRef;
  files: readonly ExtraFileView[];
  /** The section may write now (adds, CRUD). */
  canWrite: boolean;
  /** The file whose name button is the section's tab stop. */
  tabStop: number | null;
  playingPath: string | null;
  /** The e-manual stays inside the game on this system (D16). */
  manualNote: boolean;
  /** Its rows are shown (a band over 200 files starts closed). */
  open: boolean;
  onToggle(): void;
  onFocusRow(index: number): void;
  onAdd?: ((category: CategoryRef) => void) | undefined;
  onOpen?: ((index: number) => void) | undefined;
  onPlay?: ((index: number) => void) | undefined;
}

/**
 * One category band of the Extras section: a header (translated name, the
 * folder on disk in mono, the count, the Manila «Oficial» mark when it
 * holds official files, «Añadir a…») over its file rows. A band of more
 * than 200 files starts collapsed and virtualises its rows.
 */
export function ExtrasCategory({
  category,
  files,
  canWrite,
  tabStop,
  playingPath,
  manualNote,
  open,
  onToggle,
  onFocusRow,
  onAdd,
  onOpen,
  onPlay,
}: ExtrasCategoryProps) {
  const t = useT();
  const id = useId();
  const label = categoryLabel(category, t);
  const dir = diskName(category);
  const official = files.some((f) => f.provenance === 'official');
  const large = files.length > VIRTUALISE_AFTER;

  const row = (f: ExtraFileView) => ({
    file: f,
    tabStop: f.index === tabStop,
    playing: f.path === playingPath,
    onFocusRow,
    onOpen,
    onPlay,
  });

  return (
    <div
      role="group"
      aria-labelledby={`${id}-h`}
      className="extras-band"
      data-large={large || undefined}
    >
      <div className="extras-band__head">
        {large && (
          <button
            type="button"
            className="extras-band__toggle"
            aria-expanded={open}
            aria-label={t('extras.showFiles', { category: label })}
            onClick={onToggle}
          >
            <ChevronRight aria-hidden />
          </button>
        )}
        <h4 id={`${id}-h`} className="extras-band__name">
          {label}
        </h4>
        {dir && <span className="extras-band__dir mono">{dir}</span>}
        <span className="extras-band__count mono">{files.length}</span>
        {official && (
          <span className="extras-official">{t('extras.official')}</span>
        )}
        {category.kind !== 'root' && (
          <Tip content={t('extras.addTo', { category: label })}>
            <button
              type="button"
              className="extras-band__add"
              aria-label={t('extras.addTo', { category: label })}
              disabled={!canWrite}
              onClick={() => onAdd?.(category)}
            >
              <Plus aria-hidden />
            </button>
          </Tip>
        )}
      </div>
      {manualNote && (
        <p className="panel-note extras-band__note">
          {t('extras.manualInside')}
        </p>
      )}
      {open &&
        (large ? (
          <VirtualRows files={files} row={row} />
        ) : (
          <ul className="extras-list">
            {files.map((f) => (
              <ExtraRow key={f.path} {...row(f)} />
            ))}
          </ul>
        ))}
    </div>
  );
}

function VirtualRows({
  files,
  row,
}: {
  files: readonly ExtraFileView[];
  row: (
    f: ExtraFileView,
  ) => Omit<
    Parameters<typeof ExtraRow>[0],
    'measureRef' | 'style' | 'virtualIndex'
  >;
}) {
  const scroller = useRef<HTMLDivElement>(null);
  // eslint-disable-next-line react-hooks/incompatible-library -- TanStack Virtual is not memoisable by the React Compiler
  const virtualizer = useVirtualizer({
    count: files.length,
    getScrollElement: () => scroller.current,
    estimateSize: () => ROW_H,
    overscan: 8,
    getItemKey: (i) => files[i]!.path,
    initialRect: { width: 320, height: LIST_H },
  });
  return (
    <div ref={scroller} className="extras-band__scroll">
      <ul
        className="extras-list"
        style={{ height: virtualizer.getTotalSize(), position: 'relative' }}
      >
        {virtualizer.getVirtualItems().map((v) => {
          const f = files[v.index]!;
          return (
            <ExtraRow
              key={v.key}
              {...row(f)}
              virtualIndex={v.index}
              measureRef={virtualizer.measureElement}
              style={{
                position: 'absolute',
                top: 0,
                left: 0,
                width: '100%',
                transform: `translateY(${v.start}px)`,
              }}
            />
          );
        })}
      </ul>
    </div>
  );
}
