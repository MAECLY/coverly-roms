import { useVirtualizer } from '@tanstack/react-virtual';
import { ChevronRight, MoreHorizontal, Plus } from 'lucide-react';
import { useId, useRef, useState } from 'react';

import { Tip } from '@/components/library/Tip';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { useT } from '@/i18n/useT';
import type { CategoryRef, ExtraFileView } from '@/ipc/types';
import { categoryKey, categoryLabel, diskName } from '@/lib/extras';

import { ExtraRow } from './ExtraRow';
import { useExtrasActions } from './extrasContext';

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
  /** Every file of the category, whatever the filter shows. */
  total?: number | undefined;
  /** A dragged row is over this band: the drop highlight. */
  dropOver?: boolean | undefined;
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
  total,
  dropOver,
  onToggle,
  onFocusRow,
  onAdd,
  onOpen,
  onPlay,
}: ExtrasCategoryProps) {
  const t = useT();
  const id = useId();
  const a = useExtrasActions();
  const [renaming, setRenaming] = useState(false);
  const count = total ?? files.length;
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
      data-extras-band={categoryKey(category)}
      data-drop={dropOver || undefined}
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
        {renaming && category.kind === 'user' ? (
          <CategoryRename
            name={category.name}
            onDone={() => setRenaming(false)}
          />
        ) : (
          <h4 id={`${id}-h`} className="extras-band__name">
            {label}
          </h4>
        )}
        {dir && <span className="extras-band__dir mono">{dir}</span>}
        <span className="extras-band__count mono">{count}</span>
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
        {category.kind === 'user' && (
          <DropdownMenu modal={false}>
            <DropdownMenuTrigger asChild>
              <button
                type="button"
                className="extras-band__add"
                aria-label={t('extras.categoryMenu', { category: label })}
                data-extras-band-menu=""
                disabled={!a.canWrite}
              >
                <MoreHorizontal aria-hidden />
              </button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuItem onSelect={() => setRenaming(true)}>
                {t('extras.rename')}
              </DropdownMenuItem>
              <DropdownMenuItem
                variant="destructive"
                onSelect={() => a.removeCategory(category.name, count)}
              >
                {t('extras.categoryRemove')}
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
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

/** An own category's name in an input (D1): `Enter` saves, `Esc` cancels
 * without closing the panel; a refused name says why under it. */
function CategoryRename({ name, onDone }: { name: string; onDone(): void }) {
  const t = useT();
  const a = useExtrasActions();
  const [value, setValue] = useState(name);
  const [error, setError] = useState<string | null>(null);
  const errId = useId();

  async function save() {
    if (value === name) return onDone();
    const refused = await a.renameCategory(name, value);
    if (refused === null) onDone();
    else setError(refused);
  }

  return (
    <div className="extras-rename extras-rename--band">
      <input
        type="text"
        className="extras-rename__input"
        name="extras-category-name"
        // eslint-disable-next-line jsx-a11y/no-autofocus -- the user asked to rename this category
        autoFocus
        value={value}
        aria-label={t('extras.categoryName')}
        aria-invalid={error !== null || undefined}
        aria-describedby={error ? errId : undefined}
        onFocus={(e) => e.currentTarget.select()}
        onChange={(e) => {
          setValue(e.target.value);
          setError(null);
        }}
        onBlur={() => {
          if (error === null) onDone();
        }}
        onKeyDown={(e) => {
          e.stopPropagation();
          if (e.key === 'Enter') {
            e.preventDefault();
            void save();
          } else if (e.key === 'Escape') {
            e.preventDefault();
            onDone();
          }
        }}
      />
      {error && (
        <p id={errId} role="alert" className="extras-rename__error">
          {error}
        </p>
      )}
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
