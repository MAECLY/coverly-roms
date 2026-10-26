import { useContext, useEffect, useMemo, useState } from 'react';

import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import type { JobHistoryItem } from '@/ipc/types';

/** The ids of `later-jobs` as "Covers 17 Oct 2026, 10:32", once the
 * history is there (the ids themselves until then). */
export function useLaterJobs(ids: string[] | null): string {
  const t = useT();
  const locale = useContext(LocaleContext);
  const [history, setHistory] = useState<JobHistoryItem[]>([]);
  const key = ids?.join(', ') ?? null;
  useEffect(() => {
    if (key === null) return;
    let alive = true;
    void getBackend()
      .then((b) => b.jobHistory())
      .then(
        (h) => alive && setHistory(Array.isArray(h) ? h : []),
        () => undefined,
      );
    return () => {
      alive = false;
    };
  }, [key]);
  return useMemo(() => {
    if (!ids) return '';
    const dates = new Intl.DateTimeFormat(locale, {
      dateStyle: 'medium',
      timeStyle: 'short',
    });
    return ids
      .map((id) => {
        const h = history.find((x) => x.job === id);
        if (!h) return id;
        const d = new Date(h.created_at);
        return t('organize.undo.laterJob', {
          kind: t(`jobs.kind.${h.kind}`),
          date: Number.isNaN(d.getTime())
            ? t('jobs.unknownDate')
            : dates.format(d),
        });
      })
      .join(', ');
  }, [ids, history, locale, t]);
}
