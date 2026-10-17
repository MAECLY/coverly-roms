import { useContext, useMemo } from 'react';

import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import type { EntryView } from '@/ipc/types';

export function StatusBar({
  visible,
  total,
  scanMs,
}: {
  visible: readonly EntryView[];
  total: number;
  scanMs: number;
}) {
  const t = useT();
  const locale = useContext(LocaleContext);
  const n = useMemo(() => new Intl.NumberFormat(locale), [locale]);
  const missing = useMemo(
    () => visible.filter((e) => e.art.cover === 'missing').length,
    [visible],
  );
  return (
    <footer className="status-bar" aria-live="polite">
      <span>
        {t('status.shown', {
          shown: n.format(visible.length),
          total: n.format(total),
        })}
      </span>
      <span>{t('status.missingCover', { n: n.format(missing) })}</span>
      <span className="text-cr-text-secondary">
        {t('status.scan', { ms: n.format(scanMs) })}
      </span>
    </footer>
  );
}
