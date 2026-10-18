import { ChevronRight } from 'lucide-react';
import { useId, useState } from 'react';

import { Button } from '@/components/ui/button';
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible';
import { ScrollArea } from '@/components/ui/scroll-area';
import { NativeSelect } from '@/components/ui/select';
import { SlotMark } from '@/components/ui/slot-mark';
import { useT } from '@/i18n/useT';
import type { OrganizeItem } from '@/ipc/types';
import { ROWS_PER_GROUP, whyKey, type ReviewBucket } from '@/lib/organize';

import { NewBadge, NewCount, newRow, type IsNew } from './NewMarks';
import {
  chevron,
  fileList,
  fileRow,
  hintText,
  listViewport,
  panel,
  pathText,
  sectionTitle,
} from './styles';

/** A system for the items, or leave them where they are. */
export type OrganizeChoice = 'leave' | { system: string };

const LEAVE = 'leave';

interface Props {
  buckets: ReviewBucket[];
  systemName(id: string): string;
  disabled?: boolean;
  onDecide(itemIds: number[], choice: OrganizeChoice): void;
  /** Files the last "scan again" found. */
  isNew?: IsNew;
}

const toChoice = (value: string): OrganizeChoice =>
  value === LEAVE ? 'leave' : { system: value };

/**
 * Doubtful items, bucketed by candidate set so a whole bucket is settled
 * with one choice (spec §4.2.4).
 */
export function ReviewBuckets({
  buckets,
  systemName,
  disabled = false,
  onDecide,
  isNew,
}: Props) {
  const t = useT();
  return (
    <section aria-labelledby="organize-review-title" className="grid gap-2.5">
      <div className="grid gap-1">
        <h2 id="organize-review-title" className={sectionTitle}>
          {t('organize.review.title')}
        </h2>
        <p className={hintText}>{t('organize.review.hint')}</p>
      </div>
      <div className="flex flex-col gap-2">
        {buckets.map((bucket) => (
          <Bucket
            key={bucket.key}
            bucket={bucket}
            systemName={systemName}
            disabled={disabled}
            onDecide={onDecide}
            isNew={isNew}
          />
        ))}
      </div>
    </section>
  );
}

function Bucket({
  bucket,
  systemName,
  disabled,
  onDecide,
  isNew,
}: {
  bucket: ReviewBucket;
  systemName(id: string): string;
  disabled: boolean;
  onDecide(itemIds: number[], choice: OrganizeChoice): void;
  isNew?: IsNew | undefined;
}) {
  const t = useT();
  const id = useId();
  const hintId = useId();
  // Start undecided: the candidates are hints until the user picks one, so
  // nothing looks settled that would be skipped as "no system chosen".
  const [choice, setChoice] = useState('');
  const [open, setOpen] = useState(false);
  const [visible, setVisible] = useState(ROWS_PER_GROUP);
  const rest = Math.max(0, bucket.items.length - visible);
  const title = t('organize.review.bucket', {
    systems: bucket.systems.map(systemName).join(' / '),
    count: bucket.items.length,
  });
  const count = bucket.items.length;
  const fresh = isNew ? bucket.items.filter(isNew).length : 0;
  const likely = t('organize.review.likely', {
    systems: bucket.systems
      .map(systemName)
      .join(t('organize.review.likelyThen')),
  });
  return (
    <div
      role="group"
      aria-labelledby={id}
      aria-describedby={hintId}
      className={panel}
    >
      <div className="flex flex-wrap items-center gap-x-3 gap-y-2 px-3 py-2.5">
        {/* Dashed: these files have no shelf yet. */}
        <SlotMark state="missing" size={16} />
        <div className="grid min-w-0 flex-1 gap-0.5">
          <p
            id={id}
            className="m-0 flex flex-wrap items-center gap-2 text-ui font-medium"
          >
            {title}
            <NewCount count={fresh} />
          </p>
          <p id={hintId} className={hintText}>
            {likely}
          </p>
        </div>
        <NativeSelect
          size="sm"
          className="w-auto max-w-[16rem]"
          aria-labelledby={id}
          value={choice}
          disabled={disabled}
          onChange={(e) => setChoice(e.target.value)}
        >
          <option value="" disabled>
            {t('organize.review.choose')}
          </option>
          <Options systems={bucket.systems} systemName={systemName} />
        </NativeSelect>
        <Button
          variant="outline"
          size="sm"
          disabled={disabled || !choice}
          onClick={() => {
            if (!choice) return;
            onDecide(
              bucket.items.map((i) => i.id),
              toChoice(choice),
            );
          }}
        >
          {/* A verb plus the exact count (brand §7). */}
          {count === 1
            ? t('organize.review.applyOne')
            : t('organize.review.applyCount', { count })}
        </Button>
      </div>
      <Collapsible
        open={open}
        onOpenChange={setOpen}
        className="border-t border-cr-border"
      >
        <CollapsibleTrigger asChild>
          <Button
            variant="ghost"
            size="sm"
            className="group w-full justify-start rounded-none rounded-b-lg px-3 text-cr-text-secondary"
          >
            <ChevronRight aria-hidden className={chevron} />
            {t('organize.review.items', { count: bucket.items.length })}
          </Button>
        </CollapsibleTrigger>
        {/* Rows reach the DOM only once opened, capped like the groups. */}
        <CollapsibleContent className="border-t border-cr-border">
          <ScrollArea aria-label={title} viewportClassName={listViewport}>
            <ul className={fileList}>
              {bucket.items.slice(0, visible).map((item) => (
                <ReviewRow
                  key={item.id}
                  item={item}
                  systems={bucket.systems}
                  systemName={systemName}
                  disabled={disabled}
                  onDecide={onDecide}
                  fresh={isNew?.(item) ?? false}
                />
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
    </div>
  );
}

function Options({
  systems,
  systemName,
}: {
  systems: string[];
  systemName(id: string): string;
}) {
  const t = useT();
  return (
    <>
      {systems.map((s) => (
        <option key={s} value={s}>
          {systemName(s)}
        </option>
      ))}
      <option value={LEAVE}>{t('organize.review.leave')}</option>
    </>
  );
}

function ReviewRow({
  item,
  systems,
  systemName,
  disabled,
  onDecide,
  fresh,
}: {
  item: OrganizeItem;
  systems: string[];
  systemName(id: string): string;
  disabled: boolean;
  onDecide(itemIds: number[], choice: OrganizeChoice): void;
  fresh: boolean;
}) {
  const t = useT();
  const name = item.sources[0] ?? '';
  return (
    <li className={fresh ? `${fileRow} ${newRow}` : fileRow}>
      <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
        <span className={`${pathText} min-w-0 flex-1`}>{name}</span>
        {fresh && <NewBadge />}
        <NativeSelect
          size="sm"
          className="w-auto max-w-[16rem]"
          aria-label={t('organize.review.systemFor', { name })}
          value=""
          disabled={disabled}
          onChange={(e) => {
            if (e.target.value) onDecide([item.id], toChoice(e.target.value));
          }}
        >
          <option value="">{t('organize.review.choose')}</option>
          <Options systems={systems} systemName={systemName} />
        </NativeSelect>
      </div>
      {item.why.map((e, i) => {
        const why = whyKey(e, systemName);
        return (
          <p key={i} className={hintText}>
            {t(why.key, why.vars)}
          </p>
        );
      })}
    </li>
  );
}
