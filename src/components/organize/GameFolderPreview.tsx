import { ChevronRight } from 'lucide-react';
import { useMemo, useState } from 'react';

import { Button } from '@/components/ui/button';
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible';
import { useRichT } from '@/i18n/rich';
import { useT } from '@/i18n/useT';
import type { OrganizeItem, TranslatedGameView } from '@/ipc/types';
import {
  emulatorName,
  fixedFolderLabel,
  gameFolderPreview,
  itemKey,
  keyedText,
  translatedPreview,
  translatedTitle,
  type GameFolderPreview as Preview,
  type LibraryEntry,
} from '@/lib/organize';

import { chevron, hintText, pathText } from './styles';

/** Session key of a base's open state: its source path (spec §9.1). */
const storageKey = (base: OrganizeItem) =>
  `coverly.organize.gameFolder:${itemKey(base)}`;

function readOpen(key: string): boolean {
  try {
    return window.sessionStorage.getItem(key) === '1';
  } catch {
    /* blocked storage: collapsed */
    return false;
  }
}

function writeOpen(key: string, open: boolean) {
  try {
    if (open) window.sessionStorage.setItem(key, '1');
    else window.sessionStorage.removeItem(key);
  } catch {
    /* blocked storage: not remembered */
  }
}

/**
 * How a base's game folder will look once applied (game folders spec
 * §9.1): a disclosure in the base row's detail area, collapsed by default
 * and remembered for the session. Open, it is one nested list under a
 * single spine; fixed folders read in the UI language with their on-disk
 * name beside, names and paths in mono. The list is built only while open.
 *
 * With an emulator for the system (mods spec §9.1) it shows the emulator's
 * shape instead: the system folder, the emulator's own folder and what
 * stays in the library, under the same single spine.
 */
export function GameFolderPreview({
  base,
  items,
  translated,
}: {
  base: OrganizeItem;
  /** The base's system group: its linked children are among them. */
  items: readonly OrganizeItem[];
  /** The base laid out for its system's emulator, when it has one. */
  translated?: TranslatedGameView | null;
}) {
  const t = useT();
  const key = storageKey(base);
  const [open, setOpen] = useState(() => readOpen(key));
  const hasChildren = useMemo(
    () => items.some((i) => i !== base && i.game?.of === base.id),
    [items, base],
  );
  const emulator = translated ? emulatorName(translated.emulator) : null;
  if (!translated && (!base.game?.folder || !hasChildren)) return null;
  const label = emulator
    ? t('organize.gameFolder.showFor', { emulator })
    : t('organize.gameFolder.show');
  return (
    <Collapsible
      open={open}
      onOpenChange={(next) => {
        setOpen(next);
        writeOpen(key, next);
      }}
      className="grid justify-items-start gap-1"
    >
      <CollapsibleTrigger asChild>
        <Button
          variant="ghost"
          size="sm"
          className="group -ml-2.5 min-h-6 gap-1.5 text-cr-text-secondary hover:text-cr-text"
        >
          <ChevronRight aria-hidden className={chevron} />
          {open ? t('organize.gameFolder.hide') : label}
        </Button>
      </CollapsibleTrigger>
      <CollapsibleContent className="w-full">
        {open &&
          (translated && emulator ? (
            <EmulatorTree base={base} view={translated} emulator={emulator} />
          ) : (
            <Tree items={items} base={base} />
          ))}
      </CollapsibleContent>
    </Collapsible>
  );
}

function Tree({
  items,
  base,
}: {
  items: readonly OrganizeItem[];
  base: OrganizeItem;
}) {
  const t = useT();
  const preview = useMemo(() => gameFolderPreview(items, base), [items, base]);
  if (!preview) return null;
  return (
    <ul
      aria-label={t('organize.gameFolder.label', { title: preview.name })}
      className={spine}
    >
      <li className="grid gap-0.5">
        <span className="text-caption">
          <span
            className={`${pathText} text-cr-text`}
          >{`${preview.name}/`}</span>
          {preview.exists && (
            <>
              {' '}
              <span aria-hidden className="text-cr-text-secondary">
                ·
              </span>{' '}
              <span className="text-cr-text-secondary">
                {t('organize.gameFolder.exists')}
              </span>
            </>
          )}
        </span>
        <Contents preview={preview} />
      </li>
    </ul>
  );
}

const spine =
  'm-0 list-none border-0 border-l-[length:var(--cr-border-width-spine)] border-solid border-cr-border bg-cr-surface py-1.5 pl-3 pr-2';

const nested = 'm-0 grid list-none gap-0.5 p-0 pl-4';

/** A base laid out for an emulator: up to three lists under one spine,
 * each left out when empty (mods spec §9.1). */
function EmulatorTree({
  base,
  view,
  emulator,
}: {
  base: OrganizeItem;
  view: TranslatedGameView;
  emulator: string;
}) {
  const t = useT();
  const preview = useMemo(() => translatedPreview(view), [view]);
  const title = translatedTitle(base);
  return (
    <ul
      aria-label={t('organize.gameFolder.labelFor', { title, emulator })}
      className={`${spine} grid gap-1`}
    >
      <Section
        head={t('organize.translate.system')}
        paths={preview.system}
        more={preview.more.system}
      />
      <Section
        head={t('organize.translate.emulatorData', { emulator })}
        // The app never sends a data folder (IPC carries none): Coverly
        // leaves it alone and says so.
        hint={t('organize.translate.emulatorDataUnknown')}
        paths={preview.emulatorData}
        more={preview.more.emulatorData}
      />
      <LibrarySection entries={preview.library} more={preview.more.library} />
    </ul>
  );
}

/** What stays in the library: «install it in the emulator» once for the
 * list, and each other entry's own reason under its path (mods spec
 * D14, §10). */
function LibrarySection({
  entries,
  more,
}: {
  entries: readonly LibraryEntry[];
  more: number;
}) {
  const t = useT();
  const tr = useRichT();
  if (entries.length === 0) return null;
  const installs = entries.some((e) => e.why === null);
  return (
    <li className="grid gap-0.5">
      <span className="text-caption text-cr-text">
        {t('organize.translate.library')}
      </span>
      {installs && (
        <span className={hintText}>{t('organize.translate.installHere')}</span>
      )}
      <ul className={nested}>
        {entries.map(({ path, why }, i) => (
          <li key={`${i}:${path}`} className="grid gap-0.5">
            <span className={`${pathText} text-cr-text`}>{path}</span>
            {why && <span className={hintText}>{keyedText(t, why)}</span>}
          </li>
        ))}
        {more > 0 && (
          <li className={hintText}>
            {tr('organize.gameFolder.more', { count: more })}
          </li>
        )}
      </ul>
    </li>
  );
}

function Section({
  head,
  hint,
  paths,
  more,
}: {
  head: string;
  hint?: string;
  paths: readonly string[];
  more: number;
}) {
  const tr = useRichT();
  if (paths.length === 0) return null;
  return (
    <li className="grid gap-0.5">
      <span className="text-caption text-cr-text">{head}</span>
      {hint && <span className={hintText}>{hint}</span>}
      <ul className={nested}>
        {paths.map((path, i) => (
          // Two placements may share a path; their order keeps them apart.
          <li key={`${i}:${path}`} className={`${pathText} text-cr-text`}>
            {path}
          </li>
        ))}
        {more > 0 && (
          <li className={hintText}>
            {tr('organize.gameFolder.more', { count: more })}
          </li>
        )}
      </ul>
    </li>
  );
}

function Contents({ preview }: { preview: Preview }) {
  const t = useT();
  const tr = useRichT();
  return (
    <ul className={nested}>
      {preview.files.map((name) => (
        <li key={name} className={`${pathText} text-cr-text`}>
          {name}
        </li>
      ))}
      {preview.folders.map((f) => (
        <li key={f.folder} className="grid gap-0.5">
          <span className="text-caption text-cr-text-secondary">
            <span className="text-cr-text">
              {tr(
                'organize.gameFolder.count',
                {
                  folder: t(fixedFolderLabel(f.folder).key),
                  count: f.count,
                },
                ['count'],
              )}
            </span>{' '}
            <span aria-hidden>·</span>{' '}
            <span>
              {tr('organize.gameFolder.onDisk', { name: `${f.disk}/` })}
            </span>
          </span>
          <ul className={nested}>
            {f.names.map((name) => (
              <li key={name} className={`${pathText} text-cr-text`}>
                {name}
              </li>
            ))}
            {f.more > 0 && (
              <li className={hintText}>
                {tr('organize.gameFolder.more', { count: f.more })}
              </li>
            )}
          </ul>
        </li>
      ))}
    </ul>
  );
}
