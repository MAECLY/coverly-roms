import {
  Clapperboard,
  FileText,
  Image as ImageIcon,
  MoreHorizontal,
  Music,
  Package,
  Play,
} from 'lucide-react';
import { useContext, type CSSProperties, type Ref } from 'react';

import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import type { ExtraFileView, ExtraType } from '@/ipc/types';
import { officialWhatKey, splitMiddle } from '@/lib/extras';
import { formatSize } from '@/lib/format';

const GLYPH: Record<ExtraType, typeof Music> = {
  audio: Music,
  video: Clapperboard,
  image: ImageIcon,
  document: FileText,
  other: Package,
};

/** The sub-path of a file inside its category (`Disc 2/`), if any. */
function subPath(file: ExtraFileView): string {
  const parts = file.path.split('/');
  const skip = file.category.kind === 'root' ? 0 : 1;
  return parts.slice(skip, -1).join('/');
}

export interface ExtraRowProps {
  file: ExtraFileView;
  /** The row whose name button takes the section's single tab stop. */
  tabStop: boolean;
  playing: boolean;
  onFocusRow(index: number): void;
  /** Plays (audio) or shows (viewer) the file; Enter or double-click. */
  onOpen?: ((index: number) => void) | undefined;
  /** Starts the mini player on this track (audio played in the app). */
  onPlay?: ((index: number) => void) | undefined;
  /** A virtualised row: its measuring ref and position. */
  measureRef?: Ref<HTMLLIElement> | undefined;
  style?: CSSProperties | undefined;
  virtualIndex?: number | undefined;
}

/**
 * One file of a category band (extras spec §8.1, §8.4): type glyph, name
 * (the row's primary control and roving tab stop), its tags as chips on a
 * second line, the size in mono and the row's own controls. Selection,
 * rename, move and trash come with the panel CRUD.
 */
export function ExtraRow({
  file,
  tabStop,
  playing,
  onFocusRow,
  onOpen,
  onPlay,
  measureRef,
  style,
  virtualIndex,
}: ExtraRowProps) {
  const t = useT();
  const locale = useContext(LocaleContext);
  const Glyph = GLYPH[file.kind];
  const size = formatSize(file.size, locale);
  const official = file.provenance === 'official';
  const what = file.format ? officialWhatKey(file.format) : null;
  const [head, tail] = splitMiddle(file.name);
  const sub = subPath(file);
  const label = [
    file.name,
    t(`extras.kind.${file.kind}`),
    size,
    ...(file.tags.length > 0
      ? [t('extras.rowTags', { tags: file.tags.join(', ') })]
      : []),
    ...(official ? [t('extras.rowOfficial')] : []),
  ].join(', ');
  const playsHere =
    onPlay !== undefined && file.kind === 'audio' && file.playable === 'inline';
  const ctl = { 'data-extras-ctl': '', tabIndex: -1 } as const;

  return (
    <li
      ref={measureRef}
      style={style}
      data-index={virtualIndex}
      data-extras-row=""
      data-file={file.index}
      data-playing={playing || undefined}
      className="extras-row"
    >
      <Glyph className="extras-row__glyph" aria-hidden />
      <div className="extras-row__main">
        <button
          type="button"
          className="extras-row__name"
          data-extras-primary=""
          data-extras-ctl=""
          tabIndex={tabStop ? 0 : -1}
          aria-label={label}
          title={file.path}
          onFocus={() => onFocusRow(file.index)}
          onDoubleClick={() => onOpen?.(file.index)}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && onOpen) {
              e.preventDefault();
              onOpen(file.index);
            }
          }}
        >
          <span className="extras-row__head">{head}</span>
          {tail && <span className="extras-row__tail">{tail}</span>}
        </button>
        {sub && <span className="extras-row__sub mono">{sub}/</span>}
        {file.tags.length > 0 && (
          <ul className="extras-tags" aria-hidden>
            {file.tags.map((tag) => (
              <li key={tag} className="extras-tag">
                {tag}
              </li>
            ))}
          </ul>
        )}
      </div>
      <span className="extras-row__size mono">{size}</span>
      <span className="extras-row__ctls">
        {playsHere && (
          <button
            type="button"
            className="extras-row__btn"
            {...ctl}
            aria-label={t('extras.playFile', { name: file.name })}
            onClick={() => onPlay(file.index)}
          >
            <Play aria-hidden />
          </button>
        )}
        <DropdownMenu modal={false}>
          <DropdownMenuTrigger asChild>
            <button
              type="button"
              className="extras-row__btn"
              {...ctl}
              aria-label={t('extras.rowMenu', { name: file.name })}
            >
              <MoreHorizontal aria-hidden />
            </button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuLabel className="extras-menu__what">
              {official && what
                ? t('extras.officialWhat', { what: t(what) })
                : file.name}
            </DropdownMenuLabel>
          </DropdownMenuContent>
        </DropdownMenu>
      </span>
    </li>
  );
}
