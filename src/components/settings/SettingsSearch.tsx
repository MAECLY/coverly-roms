import { Search, X } from 'lucide-react';
import { forwardRef, type KeyboardEvent } from 'react';

import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Kbd, KbdGroup } from '@/components/ui/kbd';
import { useT } from '@/i18n/useT';
import { isMac, modKeyLabel } from '@/lib/keyboard';

import { MonoIn } from './Field';
import type { SettingEntry } from './registry';

export const RESULTS_ID = 'settings-results';
const optionId = (i: number) => `settings-result-${i}`;

interface FieldProps {
  query: string;
  results: readonly SettingEntry[];
  active: number;
  onQuery(query: string): void;
  onActive(active: number): void;
  onChoose(entry: SettingEntry): void;
  /** Escape in an empty field: back to the selected tab (D9). */
  onLeave(): void;
}

/** The search field at the top of the settings nav: an ARIA 1.2 combobox
 * that drives the results listbox (spec §6.3). */
export const SettingsSearchField = forwardRef<HTMLInputElement, FieldProps>(
  ({ query, results, active, onQuery, onActive, onChoose, onLeave }, ref) => {
    const t = useT();
    const n = results.length;

    function onKeyDown(ev: KeyboardEvent<HTMLInputElement>) {
      if (ev.key === 'ArrowDown' || ev.key === 'ArrowUp') {
        if (n === 0) return;
        ev.preventDefault();
        const step = ev.key === 'ArrowDown' ? 1 : -1;
        onActive((active + step + n) % n);
      } else if (ev.key === 'Enter') {
        const entry = results[active];
        if (!entry) return;
        ev.preventDefault();
        onChoose(entry);
      } else if (ev.key === 'Escape') {
        ev.preventDefault();
        if (query !== '') onQuery('');
        else onLeave();
      }
    }

    return (
      <div className="settings-search relative">
        <Search
          aria-hidden
          className="pointer-events-none absolute left-2.5 top-1/2 size-4 -translate-y-1/2 text-cr-text-secondary"
        />
        <Input
          ref={ref}
          type="search"
          name="settings-search"
          role="combobox"
          autoComplete="off"
          spellCheck={false}
          className="pl-8 pr-14 [&::-webkit-search-cancel-button]:hidden"
          aria-label={t('settings.search.label')}
          aria-expanded={query !== ''}
          aria-controls={RESULTS_ID}
          aria-autocomplete="list"
          aria-keyshortcuts={isMac() ? 'Meta+F /' : 'Control+F /'}
          aria-activedescendant={n > 0 ? optionId(active) : undefined}
          placeholder={t('settings.search.placeholder')}
          value={query}
          onChange={(e) => onQuery(e.target.value)}
          onKeyDown={onKeyDown}
        />
        {query === '' ? (
          <KbdGroup
            aria-hidden
            className="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2"
          >
            <Kbd>{modKeyLabel()}</Kbd>
            <Kbd>F</Kbd>
          </KbdGroup>
        ) : (
          <Button
            variant="ghost"
            size="icon-sm"
            className="absolute right-1 top-1/2 -translate-y-1/2"
            aria-label={t('settings.search.clear')}
            // Keep focus in the field: the click clears, typing goes on.
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => onQuery('')}
          >
            <X aria-hidden />
          </Button>
        )}
      </div>
    );
  },
);
SettingsSearchField.displayName = 'SettingsSearchField';

interface ResultsProps {
  results: readonly SettingEntry[];
  active: number;
  /** The query once typing has stopped (300 ms) and its number of
   * matches: what the live count says. */
  settled: string;
  settledCount: number;
  onActive(active: number): void;
  onChoose(entry: SettingEntry): void;
}

/** The matches, in place of the tab panel while there is a query. */
export function SettingsResults({
  results,
  active,
  settled,
  settledCount: n,
  onActive,
  onChoose,
}: ResultsProps) {
  const t = useT();
  let count: React.ReactNode = null;
  if (settled !== '') {
    if (n === 0) count = t('settings.search.none', { query: settled });
    else
      count = (
        <MonoIn
          // The number in the data face; «1 ajuste» has it as a literal.
          text={
            n === 1
              ? t('settings.search.countOne').replace('1', '\u0000')
              : t('settings.search.count', { count: '\u0000' })
          }
          value={n}
        />
      );
  }
  return (
    <div className="settings-results min-w-0 flex-1 overflow-y-auto px-6 pb-8 pt-5">
      <p
        aria-live="polite"
        data-testid="settings-search-count"
        className="settings-results__count"
      >
        {count}
      </p>
      <ul
        role="listbox"
        id={RESULTS_ID}
        aria-label={t('settings.search.label')}
      >
        {results.map((e, i) => (
          // The field owns the keys (combobox, aria-activedescendant).
          // eslint-disable-next-line jsx-a11y/click-events-have-key-events
          <li
            key={e.id}
            id={optionId(i)}
            role="option"
            aria-selected={i === active}
            className="settings-results__item"
            onMouseDown={(ev) => ev.preventDefault()}
            onMouseMove={() => i !== active && onActive(i)}
            onClick={() => onChoose(e)}
          >
            <span className="settings-results__label">{t(e.label)}</span>
            <span className="settings-results__where">
              {e.fold
                ? t('settings.search.where', {
                    section: t(`settings.section.${e.section}`),
                    fold: t('settings.advancedOptions'),
                  })
                : t(`settings.section.${e.section}`)}
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}
