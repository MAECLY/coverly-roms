import {
  Clapperboard,
  FileText,
  Image as ImageIcon,
  MoreHorizontal,
  Music,
  Package,
  Play,
} from 'lucide-react';
import {
  useContext,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent,
  type MouseEvent,
  type Ref,
} from 'react';

import { Checkbox } from '@/components/ui/checkbox';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuShortcut,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import type { ExtraFileView, ExtraType } from '@/ipc/types';
import {
  categoryKey,
  categoryLabel,
  officialWhatKey,
  sameCategory,
  splitExt,
  splitMiddle,
} from '@/lib/extras';
import { formatSize } from '@/lib/format';
import { isTyping, letterOf } from '@/lib/keyboard';

import { ExtraTagsPopover } from './ExtraTagsPopover';
import { useExtrasActions } from './extrasContext';

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
 * One file of a category band (extras spec §8.1, §8.3, §8.4): a checkbox
 * that takes the glyph's place on hover, focus or while anything is
 * selected; the name (the row's primary control and roving tab stop, or
 * the inline rename); its tags as chips; the size in mono and the row's
 * controls. Keys on the name: `F2` rename, `T` tags, `Alt+↑/↓` move,
 * `Space` select, `Delete` to the Trash.
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
  const a = useExtrasActions();
  const Glyph = GLYPH[file.kind];
  const size = formatSize(file.size, locale);
  const official = file.provenance === 'official';
  const what = file.format ? officialWhatKey(file.format) : null;
  const [head, tail] = splitMiddle(file.name);
  const sub = subPath(file);
  const selected = a.selection.has(file.index);
  const renaming = a.renaming === file.index;
  const nameRef = useRef<HTMLButtonElement>(null);
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
  const elsewhere = a.targets.filter((c) => !sameCategory(c, file.category));
  /** The files a key or menu acts on: the selection when this row is in
   * it, else this row alone. */
  const group = () =>
    selected && a.selection.size > 1 ? [...a.selection] : [file.index];

  function onKeyDown(e: KeyboardEvent<HTMLButtonElement>) {
    if (isTyping(e.target)) return;
    const plain = !e.altKey && !e.metaKey && !e.ctrlKey && !e.shiftKey;
    const own = (run: () => void) => {
      e.preventDefault();
      e.stopPropagation();
      run();
    };
    if (e.key === 'Enter' && plain && onOpen) own(() => onOpen(file.index));
    else if (e.key === 'F2' && plain && a.canWrite)
      own(() => a.startRename(file.index));
    else if (letterOf(e) === 't' && plain && a.canWrite)
      own(() => a.openTags(file.index));
    else if (
      (e.key === 'ArrowUp' || e.key === 'ArrowDown') &&
      e.altKey &&
      !e.metaKey &&
      !e.ctrlKey &&
      a.canWrite
    )
      own(() => a.step(file, e.key === 'ArrowUp' ? -1 : 1));
    else if (e.key === ' ' && !e.altKey && !e.metaKey && !e.ctrlKey)
      own(() => a.select(file.index, e.shiftKey ? 'range' : 'toggle'));
    else if ((e.key === 'Delete' || e.key === 'Backspace') && a.canWrite)
      own(() => a.trash(group()));
    else if (e.key === 'Escape' && a.selection.size > 0 && plain)
      own(() => a.clearSelection());
  }

  function onClick(e: MouseEvent<HTMLButtonElement>) {
    if (a.swallowClick()) return;
    const mode =
      e.metaKey || e.ctrlKey ? 'toggle' : e.shiftKey ? 'range' : 'only';
    a.select(file.index, mode);
  }

  const name = renaming ? (
    <RenameInput file={file} />
  ) : (
    <button
      ref={nameRef}
      type="button"
      className="extras-row__name"
      data-extras-primary=""
      data-extras-ctl=""
      tabIndex={tabStop ? 0 : -1}
      aria-label={label}
      title={file.path}
      onFocus={() => onFocusRow(file.index)}
      onClick={onClick}
      onDoubleClick={() => onOpen?.(file.index)}
      onKeyDown={onKeyDown}
      onKeyUp={(e) => {
        // Space toggled the checkbox on keydown: no click on keyup.
        if (e.key === ' ') e.preventDefault();
      }}
    >
      <span className="extras-row__head">{head}</span>
      {tail && <span className="extras-row__tail">{tail}</span>}
    </button>
  );

  return (
    <ExtraTagsPopover
      name={file.name}
      tags={file.tags}
      suggestions={a.suggestions}
      open={a.tagsFor === file.index}
      onOpenChange={(open) => a.openTags(open ? file.index : null)}
      onSave={(tags) => a.saveTags(file, tags)}
      onReturnFocus={() => nameRef.current?.focus()}
    >
      <li
        ref={measureRef}
        style={style}
        data-index={virtualIndex}
        data-extras-row=""
        data-file={file.index}
        data-path={file.path}
        data-playing={playing || undefined}
        data-selected={selected || undefined}
        data-landed={a.landed === file.path || undefined}
        className="extras-row"
        onPointerDown={(e) => {
          if (!renaming) a.pointerDown(e, file);
        }}
      >
        <span className="extras-row__lead">
          <Glyph className="extras-row__glyph" aria-hidden />
          <Checkbox
            className="extras-row__check"
            {...ctl}
            checked={selected}
            aria-label={t('extras.selectFile', { name: file.name })}
            onClick={(e) => {
              e.preventDefault();
              a.select(file.index, e.shiftKey ? 'range' : 'toggle');
            }}
          />
        </span>
        <div className="extras-row__main">
          {name}
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
              <DropdownMenuSeparator />
              {onOpen && (
                // The row's primary action, also for those who never
                // double-click or press Enter on it.
                <DropdownMenuItem onSelect={() => onOpen(file.index)}>
                  {file.kind === 'audio' && file.playable === 'inline'
                    ? t('player.play')
                    : t('extras.open')}
                  <DropdownMenuShortcut aria-hidden>↵</DropdownMenuShortcut>
                </DropdownMenuItem>
              )}
              <DropdownMenuItem
                disabled={!a.canWrite}
                onSelect={() => a.startRename(file.index)}
              >
                {t('extras.rename')}
                <DropdownMenuShortcut>F2</DropdownMenuShortcut>
              </DropdownMenuItem>
              <DropdownMenuSub>
                <DropdownMenuSubTrigger disabled={!a.canWrite}>
                  {t('extras.moveTo')}
                </DropdownMenuSubTrigger>
                <DropdownMenuSubContent>
                  {elsewhere.map((c) => (
                    <DropdownMenuItem
                      key={categoryKey(c)}
                      onSelect={() => a.move(group(), c)}
                    >
                      {categoryLabel(c, t)}
                    </DropdownMenuItem>
                  ))}
                </DropdownMenuSubContent>
              </DropdownMenuSub>
              <DropdownMenuItem
                disabled={!a.canWrite}
                onSelect={() => a.openTags(file.index)}
              >
                {t('extras.tags')}
                <DropdownMenuShortcut>T</DropdownMenuShortcut>
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem
                variant="destructive"
                disabled={!a.canWrite}
                onSelect={() => a.trash(group())}
              >
                {t('extras.trash')}
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </span>
      </li>
    </ExtraTagsPopover>
  );
}

/** The inline rename (D14): the stem in an input, the extension a fixed
 * mono suffix. `Enter` saves, `Esc` cancels (and never closes the panel). */
function RenameInput({ file }: { file: ExtraFileView }) {
  const t = useT();
  const a = useExtrasActions();
  const [stem, ext] = splitExt(file.name);
  const [value, setValue] = useState(stem);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  async function save() {
    if (value === stem) {
      a.startRename(null, true);
      return;
    }
    setSaving(true);
    const refused = await a.rename(file, value);
    setSaving(false);
    setError(refused);
  }

  return (
    <div className="extras-rename">
      <div className="extras-rename__field">
        <input
          type="text"
          className="extras-rename__input"
          // eslint-disable-next-line jsx-a11y/no-autofocus -- the user asked to rename this row
          autoFocus
          value={value}
          // Read-only, not disabled, while saving: a disabled input drops
          // the focus, and a refused name is typed again in place.
          readOnly={saving}
          aria-busy={saving || undefined}
          aria-label={t('extras.renameLabel', { name: file.name })}
          aria-invalid={error !== null || undefined}
          aria-describedby={error ? `extras-rename-${file.index}` : undefined}
          onFocus={(e) => e.currentTarget.select()}
          onChange={(e) => {
            setValue(e.target.value);
            setError(null);
          }}
          onBlur={() => {
            if (!saving && error === null) a.startRename(null);
          }}
          onKeyDown={(e) => {
            e.stopPropagation();
            if (e.key === 'Enter') {
              e.preventDefault();
              if (!saving) void save();
            } else if (e.key === 'Escape') {
              e.preventDefault();
              a.startRename(null, true);
            }
          }}
        />
        {ext && <span className="extras-rename__ext mono">{ext}</span>}
      </div>
      {error && (
        <p
          id={`extras-rename-${file.index}`}
          role="alert"
          className="extras-rename__error"
        >
          {error}
        </p>
      )}
    </div>
  );
}
