import { Check } from 'lucide-react';
import { useContext, useEffect, useId, useMemo, useRef, useState } from 'react';

import { Button } from '@/components/ui/button';
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group';
import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import { CONTENT_KEYS, contentActive, type ContentKey } from '@/lib/filters';
import { cn, focusRing } from '@/lib/utils';
import { useFilterStore } from '@/stores/filterStore';

import { Tip } from './Tip';

/** The pause before the result is announced: a run of presses says it once. */
const ANNOUNCE_MS = 500;

const CHIP = cn(
  'group inline-flex h-control-sm select-none items-center gap-1.5 whitespace-nowrap rounded-full border border-cr-border bg-cr-surface pl-2 pr-2.5 text-ui text-cr-text',
  'transition-colors duration-fast ease-standard hover:border-cr-border-strong',
  'aria-pressed:border-cr-primary aria-pressed:bg-cr-primary-subtle aria-pressed:text-cr-primary-subtle-fg',
  focusRing,
);

/**
 * «Mostrar juegos que» (addon views D14): four chips above the views,
 * combined with AND and with the filter panel, each with how many games
 * of the scope it keeps. Pressed state is `aria-pressed` and a check, not
 * only colour. A polite live region says how many games are left, once,
 * 500 ms after the last press (never while typing a search). Without a
 * title list (3.1), «Le falta la última actualización» says it compares
 * only within the library, as visible text that also describes the chip.
 * Hidden when the library holds no updates, DLC or demos and no chip is on.
 */
export function ContentFilters({
  counts,
  shown,
  titleList = false,
}: {
  /** Games of the scope each chip keeps on its own. */
  counts: Readonly<Record<ContentKey, number>>;
  /** Games on screen after every filter: what the announcement says. */
  shown: number;
  /** An offline title list knows the newest versions (3.1; none yet). */
  titleList?: boolean;
}) {
  const t = useT();
  const locale = useContext(LocaleContext);
  const n = useMemo(() => new Intl.NumberFormat(locale), [locale]);
  const labelId = useId();
  const hintId = useId();
  const groupRef = useRef<HTMLDivElement>(null);

  // The count is read when the timer fires, after the views caught up.
  const shownRef = useRef(shown);
  useEffect(() => {
    shownRef.current = shown;
  }, [shown]);
  const content = useFilterStore((s) => s.filters.content);
  const toggleContent = useFilterStore((s) => s.toggleContent);
  const clearContent = useFilterStore((s) => s.clearContent);
  // With every chip off the result is not «con estos filtros».
  const anyOn = contentActive(content);
  const anyOnRef = useRef(anyOn);
  useEffect(() => {
    anyOnRef.current = anyOn;
  }, [anyOn]);
  const [presses, setPresses] = useState(0);
  const [message, setMessage] = useState('');
  // Emptied on each press so a result equal to the last one is said again.
  const announceLater = () => {
    setMessage('');
    setPresses((p) => p + 1);
  };
  useEffect(() => {
    if (presses === 0) return;
    const id = window.setTimeout(() => {
      const left = shownRef.current;
      setMessage(
        !anyOnRef.current
          ? left === 1
            ? t('content.filters.clearedOne')
            : t('content.filters.cleared', { n: n.format(left) })
          : left === 0
            ? t('content.filters.none')
            : left === 1
              ? t('content.filters.resultOne')
              : t('content.filters.result', { n: n.format(left) }),
      );
    }, ANNOUNCE_MS);
    return () => window.clearTimeout(id);
  }, [presses, t, n]);

  if (!anyOn && CONTENT_KEYS.every((k) => counts[k] === 0)) return null;

  const pressed = CONTENT_KEYS.filter((k) => content[k]);
  const onValueChange = (next: string[]) => {
    for (const k of CONTENT_KEYS) {
      if (next.includes(k) !== content[k]) toggleContent(k);
    }
    announceLater();
  };

  const chip = (key: ContentKey) => {
    const describedBy = key === 'missing' && !titleList ? hintId : undefined;
    const item = (
      <ToggleGroupItem
        key={key}
        value={key}
        className={CHIP}
        {...(describedBy && { 'aria-describedby': describedBy })}
      >
        <span
          aria-hidden="true"
          className="grid size-3 place-items-center rounded-[2px] border border-dashed border-cr-slot-missing group-aria-pressed:border-solid group-aria-pressed:border-cr-primary group-aria-pressed:bg-cr-primary"
        >
          <Check
            className="hidden size-2.5 text-cr-primary-fg group-aria-pressed:block"
            strokeWidth={3}
          />
        </span>
        {t(`content.filters.${key}`)}
        <span className="mono tabular-nums text-caption text-cr-text-secondary group-aria-pressed:text-inherit">
          {n.format(counts[key])}
        </span>
      </ToggleGroupItem>
    );
    return describedBy ? (
      <Tip key={key} content={t('content.filters.missingHint')}>
        {item}
      </Tip>
    ) : (
      item
    );
  };

  return (
    <div className="flex flex-wrap items-center gap-x-2 gap-y-1.5 border-b border-cr-border bg-cr-surface px-3 py-2">
      <span id={labelId} className="text-ui text-cr-text-secondary">
        {t('content.filters.label')}
      </span>
      <ToggleGroup
        ref={groupRef}
        type="multiple"
        role="group"
        value={pressed}
        onValueChange={onValueChange}
        aria-labelledby={labelId}
        className="flex-wrap gap-1.5"
      >
        {CONTENT_KEYS.map(chip)}
      </ToggleGroup>
      {anyOn && (
        <Button
          variant="ghost"
          size="sm"
          onClick={() => {
            clearContent();
            announceLater();
            groupRef.current?.querySelector<HTMLElement>('button')?.focus();
          }}
        >
          {t('filters.clear')}
        </Button>
      )}
      {!titleList && (
        <p id={hintId} className="text-caption text-cr-text-muted">
          {t('content.filters.missingHint')}
        </p>
      )}
      <span aria-live="polite" className="sr-only">
        {message}
      </span>
    </div>
  );
}
