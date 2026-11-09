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
import type { OrganizeItem } from '@/ipc/types';
import {
  fixedFolderLabel,
  gameFolderPreview,
  itemKey,
  type GameFolderPreview as Preview,
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
 */
export function GameFolderPreview({
  base,
  items,
}: {
  base: OrganizeItem;
  /** The base's system group: its linked children are among them. */
  items: readonly OrganizeItem[];
}) {
  const t = useT();
  const key = storageKey(base);
  const [open, setOpen] = useState(() => readOpen(key));
  const hasChildren = useMemo(
    () => items.some((i) => i !== base && i.game?.of === base.id),
    [items, base],
  );
  if (!base.game?.folder || !hasChildren) return null;
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
          {t(open ? 'organize.gameFolder.hide' : 'organize.gameFolder.show')}
        </Button>
      </CollapsibleTrigger>
      <CollapsibleContent className="w-full">
        {open && <Tree items={items} base={base} />}
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
      className="m-0 list-none border-0 border-l-[length:var(--cr-border-width-spine)] border-solid border-cr-border bg-cr-surface py-1.5 pl-3 pr-2"
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

const nested = 'm-0 grid list-none gap-0.5 p-0 pl-4';

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
              {t('organize.gameFolder.count', {
                folder: t(fixedFolderLabel(f.folder).key),
                count: f.count,
              })}
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
                {t('organize.gameFolder.more', { count: f.more })}
              </li>
            )}
          </ul>
        </li>
      ))}
    </ul>
  );
}
