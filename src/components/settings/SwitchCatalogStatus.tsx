import { useContext, type ReactNode } from 'react';

import { LocaleContext } from '@/i18n/core';
import { useRichT } from '@/i18n/rich';
import type { CatalogStatus } from '@/ipc/types';
import { formatDecimalSize } from '@/lib/format';

import { errorText, hintText } from './Field';

const KEY = 'integrations.switchCatalog';

/** `2026-10-30` as the UI language writes a date (`Oct 30, 2026`), read
 * as a calendar day so no time zone moves it. */
function catalogDate(iso: string, locale: string): string {
  const d = new Date(`${iso}T00:00:00Z`);
  return Number.isNaN(d.getTime())
    ? iso
    : new Intl.DateTimeFormat(locale, {
        dateStyle: 'medium',
        timeZone: 'UTC',
      }).format(d);
}

/** The catalogue's status line (spec §7, §10): a polite live region, with
 * counts, sizes and dates in the data face. `null` while it loads. While a
 * download runs the region is busy: progress arrives up to four times a
 * second and the progress bar carries it, so a screen reader hears the
 * line once the download ends instead of every byte count. */
export function SwitchCatalogStatusLine({
  status,
}: {
  status: CatalogStatus | null;
}) {
  const tr = useRichT();
  const locale = useContext(LocaleContext);
  const size = (n: number) => formatDecimalSize(n, locale);
  const date = (iso: string) => catalogDate(iso, locale);
  let tone = hintText;
  let text: ReactNode = null;
  switch (status?.state) {
    case undefined:
      break;
    case 'off':
      text = tr(`${KEY}.statusOff`);
      break;
    case 'pending':
      text = tr(`${KEY}.pending`);
      break;
    case 'downloading':
      text =
        status.total === null
          ? tr(`${KEY}.downloadingUnknown`, { done: size(status.done) })
          : tr(`${KEY}.downloading`, {
              done: size(status.done),
              total: size(status.total),
            });
      break;
    case 'ready':
      text = tr(status.fallback ? `${KEY}.statusFallback` : `${KEY}.status`, {
        count: new Intl.NumberFormat(locale).format(status.count),
        date: date(status.updated),
      });
      break;
    case 'failed-old':
      tone = 'm-0 text-caption text-cr-warning';
      text = tr(`${KEY}.failedOld`, { date: date(status.updated) });
      break;
    case 'failed-none':
      tone = 'm-0 text-caption text-cr-warning';
      text = tr(`${KEY}.failedNone`);
      break;
    case 'rejected':
      tone = 'm-0 text-caption text-cr-warning';
      text = tr(`${KEY}.rejected`);
      break;
    case 'remove-failed':
      tone = errorText;
      text = tr(`${KEY}.removeFailed`);
      break;
  }
  return (
    <p
      role="status"
      aria-busy={status?.state === 'downloading' || undefined}
      data-testid="switch-catalog-status"
      className={`${tone} min-h-[1lh]`}
    >
      {text}
    </p>
  );
}
