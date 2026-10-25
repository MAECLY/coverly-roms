import { Archive, ChevronRight, Folder } from 'lucide-react';
import { useContext, useId, useState } from 'react';

import { SystemIcon } from '@/components/system/SystemIcon';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible';
import { ScrollArea } from '@/components/ui/scroll-area';
import { NativeSelect } from '@/components/ui/select';
import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import type {
  OrganizeItem,
  ProfileFile,
  SystemInfo,
  TransferMode,
} from '@/ipc/types';
import { formatDecimalSize } from '@/lib/format';
import {
  ROWS_PER_GROUP,
  actionOf,
  confidenceKey,
  extractDetail,
  rowNotes,
  targetDir,
  whyKey,
  type SystemGroup,
} from '@/lib/organize';

import { ExtractDetail } from './ExtractDetail';
import { NewBadge, NewCount, newRow, type IsNew } from './NewMarks';
import type { OrganizeChoice } from './ReviewBuckets';
import {
  chevron,
  fileList,
  fileRow,
  groupTrigger,
  hintText,
  listViewport,
  panel,
  pathText,
} from './styles';

const LEAVE = '__leave__';

interface Props {
  groups: SystemGroup[];
  /** Display name of the target layout (for "no folder on device"). */
  layoutName: string;
  systemName(id: string): string;
  /** Every known system: a system the user chose can be changed again. */
  systems?: SystemInfo[];
  disabled?: boolean;
  onDecide?(itemIds: number[], choice: OrganizeChoice): void;
  /** Files the last "scan again" found. */
  isNew?: IsNew;
  /** The plan's transfer mode: what applying does with as-is items. */
  mode?: TransferMode;
  /** The target layout, for where a disc's tracks go. */
  profile?: Pick<ProfileFile, 'disc_layout'> | null;
}

interface RowProps {
  layoutName: string;
  mode?: TransferMode;
  profile?: Pick<ProfileFile, 'disc_layout'> | null;
  systemName(id: string): string;
  systems?: SystemInfo[];
  disabled?: boolean;
  onDecide?(itemIds: number[], choice: OrganizeChoice): void;
  isNew?: IsNew;
}

/** The items that move, one collapsible per system folder (spec §4.2.3). */
export function SystemGroups({ groups, ...row }: Props) {
  const total = groups.reduce((n, g) => n + g.items.length, 0);
  // Large piles start closed so thousands of rows never reach the DOM at once.
  const startOpen = total <= ROWS_PER_GROUP;
  return (
    <div className="flex flex-col gap-2">
      {groups.map((g) => (
        <Group key={g.folder} group={g} startOpen={startOpen} {...row} />
      ))}
    </div>
  );
}

function Group({
  group,
  startOpen,
  ...row
}: RowProps & {
  group: SystemGroup;
  startOpen: boolean;
}) {
  const t = useT();
  const id = useId();
  const [open, setOpen] = useState(startOpen);
  const [visible, setVisible] = useState(ROWS_PER_GROUP);
  const rows = group.items.slice(0, visible);
  const rest = group.items.length - rows.length;
  const dir = targetDir(group.items[0]!);
  const isNew = row.isNew;
  const fresh = isNew ? group.items.filter(isNew).length : 0;
  return (
    <Collapsible
      open={open}
      onOpenChange={setOpen}
      role="group"
      aria-labelledby={id}
      className={panel}
    >
      <CollapsibleTrigger asChild>
        <Button id={id} variant="ghost" className={groupTrigger}>
          <ChevronRight aria-hidden className={chevron} />
          <Folder aria-hidden className="text-cr-text-secondary" />
          <span className="font-mono text-ui text-cr-text [overflow-wrap:anywhere]">
            {`${dir}/ (${group.items.length})`}
          </span>
          <SystemIcon systemId={group.systemId} size={16} />
          <span className="text-caption font-normal text-cr-text-secondary">
            {row.systemName(group.systemId)}
          </span>
          <NewCount count={fresh} />
        </Button>
      </CollapsibleTrigger>
      {/* Rows mount only while open (Radix unmounts closed content). */}
      <CollapsibleContent className="border-t border-cr-border">
        <ScrollArea aria-label={`${dir}/`} viewportClassName={listViewport}>
          <ul className={fileList}>
            {rows.map((item) => (
              <Row key={item.id} item={item} {...row} />
            ))}
          </ul>
          {rest > 0 && (
            <div className="border-t border-cr-border px-3 py-2">
              <Button
                variant="ghost"
                size="sm"
                onClick={() => setVisible((v) => v + ROWS_PER_GROUP)}
              >
                {t('organize.showMore', { count: rest })}
              </Button>
            </div>
          )}
        </ScrollArea>
      </CollapsibleContent>
    </Collapsible>
  );
}

function Row({
  item,
  layoutName,
  systemName,
  systems,
  disabled = false,
  onDecide,
  isNew,
  mode = 'move',
  profile,
}: RowProps & { item: OrganizeItem }) {
  const t = useT();
  // A manual choice is the user's decision, not a detection level.
  const confidence =
    item.method === 'user' ? null : confidenceKey(item.confidence);
  const why = item.why[0];
  const reason = why ? whyKey(why, systemName, item.disc) : null;
  const name = item.sources[0] ?? '';
  // A system the user chose (now or remembered) stays changeable: a slip in
  // the review list must not need starting over.
  const changeable =
    item.method === 'user' && !!onDecide && !!systems && !!item.system_id;
  const fresh = isNew?.(item) ?? false;
  const serials = item.disc?.serials.join(', ') ?? '';
  const notes = rowNotes(t, item, layoutName, systemName);
  return (
    <li className={fresh ? `${fileRow} ${newRow}` : fileRow}>
      <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
        <span className="grid min-w-0 flex-[1_1_16rem] gap-0.5">
          <span className={pathText}>{`${name} → ${item.target ?? ''}`}</span>
          {serials && (
            <span className="font-mono text-caption tabular-nums text-cr-text-secondary">
              {serials}
            </span>
          )}
        </span>
        {fresh && <NewBadge />}
        <ActionBadge item={item} mode={mode} />
        {confidence && (
          <Badge
            variant={
              confidence === 'organize.confidence.high' ? 'success' : 'info'
            }
            title={t('organize.confidence.label')}
          >
            {t(confidence)}
          </Badge>
        )}
        {changeable && <SystemIcon systemId={item.system_id} size={16} />}
        {changeable && (
          <NativeSelect
            size="sm"
            className="w-auto max-w-[16rem]"
            aria-label={t('organize.review.systemFor', { name })}
            value={item.system_id ?? ''}
            disabled={disabled}
            onChange={(e) => {
              const v = e.target.value;
              if (!v || v === item.system_id) return;
              onDecide([item.id], v === LEAVE ? 'leave' : { system: v });
            }}
          >
            {systems.map((s) => (
              <option key={s.id} value={s.id}>
                {s.name}
              </option>
            ))}
            <option value={LEAVE}>{t('organize.review.leave')}</option>
          </NativeSelect>
        )}
      </div>
      {reason && <p className={hintText}>{t(reason.key, reason.vars)}</p>}
      {notes.inline.map((line) => (
        <p key={line} className={hintText}>
          {line}
        </p>
      ))}
      {/* An extract row folds the archive's own facts into its detail. */}
      <ExtractDetail
        item={item}
        name={name}
        profile={profile}
        notes={notes.detail}
      />
    </li>
  );
}

/**
 * What applying does with a row, in words: *Move* / *Copy*, or *Extract* as
 * a petrol outline with the archive icon followed by its file count and
 * bytes in mono. The word is always there; colour only repeats it.
 */
function ActionBadge({
  item,
  mode,
}: {
  item: OrganizeItem;
  mode: TransferMode;
}) {
  const t = useT();
  const locale = useContext(LocaleContext);
  const action = actionOf(item, mode);
  const detail = extractDetail(item);
  if (action !== 'extract')
    return <Badge variant="neutral">{t(`organize.action.${action}`)}</Badge>;
  return (
    <span className="inline-flex items-center gap-2">
      <Badge variant="outline" className="border-primary text-primary">
        <Archive aria-hidden />
        {t('organize.action.extract')}
      </Badge>
      {detail && (
        <span className="whitespace-nowrap font-mono text-caption tabular-nums text-cr-text-secondary">
          {t('organize.extract.detail', {
            count: detail.count,
            bytes: formatDecimalSize(detail.bytes, locale),
          })}
        </span>
      )}
    </span>
  );
}
