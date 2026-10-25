import { ChevronRight } from 'lucide-react';
import { useContext } from 'react';

import { Button } from '@/components/ui/button';
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible';
import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import type { OrganizeItem, ProfileFile } from '@/ipc/types';
import { formatSize } from '@/lib/format';
import { layoutNote } from '@/lib/organize';

import { chevron, hintText, pathText } from './styles';

interface Props {
  item: OrganizeItem;
  /** The target layout: where a disc's tracks go (absent = flat). */
  profile: Pick<ProfileFile, 'disc_layout'> | null | undefined;
  /** The row's note lines, already worded (`itemNotes`). */
  notes: string[];
}

function fileName(path: string): string {
  return path.slice(
    Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\')) + 1,
  );
}

/**
 * The expanded part of an extract row (spec §11.1): every file written out
 * of the archive with its size, where they go and the archive's notes.
 * Nothing for an item that moves as it is.
 */
export function ExtractDetail({ item, profile, notes }: Props) {
  const t = useT();
  const locale = useContext(LocaleContext);
  const transfer = item.transfer;
  if (transfer.kind !== 'extract') return null;
  const label = t('organize.extract.files');
  return (
    <Collapsible className="grid gap-1">
      <CollapsibleTrigger asChild>
        <Button
          variant="ghost"
          size="sm"
          className="group -ml-1.5 w-fit gap-1 px-1.5 text-caption text-cr-text-secondary"
        >
          <ChevronRight aria-hidden className={chevron} />
          {label}
        </Button>
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className="grid gap-2 rounded-md border border-cr-border bg-cr-surface-sunken px-3 py-2">
          <ul aria-label={label} className="m-0 grid list-none gap-0.5 p-0">
            {transfer.entries.map((e) => (
              <li
                key={e.index}
                className="grid grid-cols-[minmax(0,1fr)_auto] items-baseline gap-3"
              >
                {/* The name written to the card; the folder is in the
                    layout line below, the archive's own path in the title. */}
                <span className={`${pathText} text-cr-text`} title={e.name}>
                  {fileName(e.target || e.name)}
                </span>
                <span className="text-right font-mono text-caption tabular-nums text-cr-text-secondary">
                  {formatSize(e.size, locale)}
                </span>
              </li>
            ))}
          </ul>
          <div className="grid gap-0.5">
            {layoutNote(item, profile).map((line) => (
              <p key={line.key} className={hintText}>
                {t(line.key, line.vars)}
              </p>
            ))}
            {notes.map((line) => (
              <p key={line} className={hintText}>
                {line}
              </p>
            ))}
          </div>
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}
