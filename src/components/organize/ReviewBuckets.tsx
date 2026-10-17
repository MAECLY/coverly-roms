import { useId, useState } from 'react';

import { useT } from '@/i18n/useT';
import type { OrganizeItem } from '@/ipc/types';
import { whyKey, type ReviewBucket } from '@/lib/organize';

/** A system for the items, or leave them where they are. */
export type OrganizeChoice = 'leave' | { system: string };

const LEAVE = 'leave';

interface Props {
  buckets: ReviewBucket[];
  systemName(id: string): string;
  disabled?: boolean;
  onDecide(itemIds: number[], choice: OrganizeChoice): void;
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
}: Props) {
  const t = useT();
  return (
    <section aria-labelledby="organize-review-title" className="recent">
      <h2 id="organize-review-title" className="section-title">
        {t('organize.review.title')}
      </h2>
      <p className="hint">{t('organize.review.hint')}</p>
      <div className="flex flex-col gap-3">
        {buckets.map((bucket) => (
          <Bucket
            key={bucket.key}
            bucket={bucket}
            systemName={systemName}
            disabled={disabled}
            onDecide={onDecide}
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
}: {
  bucket: ReviewBucket;
  systemName(id: string): string;
  disabled: boolean;
  onDecide(itemIds: number[], choice: OrganizeChoice): void;
}) {
  const t = useT();
  const id = useId();
  const [choice, setChoice] = useState(bucket.systems[0] ?? LEAVE);
  const title = t('organize.review.bucket', {
    systems: bucket.systems.map(systemName).join(' / '),
    count: bucket.items.length,
  });
  return (
    <div role="group" aria-labelledby={id}>
      <p id={id}>{title}</p>
      <div className="flex flex-wrap items-center gap-3">
        <select
          className="select"
          aria-labelledby={id}
          value={choice}
          disabled={disabled}
          onChange={(e) => setChoice(e.target.value)}
        >
          <Options systems={bucket.systems} systemName={systemName} />
        </select>
        <button
          type="button"
          className="ghost-button"
          disabled={disabled}
          onClick={() =>
            onDecide(
              bucket.items.map((i) => i.id),
              toChoice(choice),
            )
          }
        >
          {t('organize.review.applyAll')}
        </button>
      </div>
      <details>
        <summary>
          {t('organize.review.items', { count: bucket.items.length })}
        </summary>
        <ul className="recent-list">
          {bucket.items.map((item) => (
            <ReviewRow
              key={item.id}
              item={item}
              systems={bucket.systems}
              systemName={systemName}
              disabled={disabled}
              onDecide={onDecide}
            />
          ))}
        </ul>
      </details>
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
}: {
  item: OrganizeItem;
  systems: string[];
  systemName(id: string): string;
  disabled: boolean;
  onDecide(itemIds: number[], choice: OrganizeChoice): void;
}) {
  const t = useT();
  const name = item.sources[0] ?? '';
  return (
    <li>
      <span>{name}</span>{' '}
      <select
        className="select"
        aria-label={t('organize.review.systemFor', { name })}
        value=""
        disabled={disabled}
        onChange={(e) => {
          if (e.target.value) onDecide([item.id], toChoice(e.target.value));
        }}
      >
        <option value="">{t('organize.review.choose')}</option>
        <Options systems={systems} systemName={systemName} />
      </select>
      {item.why.map((e, i) => {
        const why = whyKey(e, systemName);
        return (
          <p key={i} className="hint">
            {t(why.key, why.vars)}
          </p>
        );
      })}
    </li>
  );
}
