import { useMemo, useState } from 'react';

import { useT } from '@/i18n/useT';
import { REGIONS, type LibraryView } from '@/ipc/types';
import {
  ART_FILTERS,
  dayEnd,
  dayStart,
  toDateInput,
  type ArtFilter,
  type Facets,
} from '@/lib/filters';
import { useFilterStore } from '@/stores/filterStore';

const MB = 1024 * 1024;
const toMb = (bytes: number | null) =>
  bytes === null ? '' : String(Math.round((bytes / MB) * 100) / 100);
function toBytes(value: string): number | null {
  const n = Number.parseFloat(value);
  return Number.isFinite(n) && n >= 0 ? Math.round(n * MB) : null;
}

/**
 * A size bound in MB. Keeps the text being typed ("1.0", "0.0") instead of
 * echoing back the rounded store value, which would rewrite the field
 * mid-entry; re-syncs only when the store changes from elsewhere (Clear).
 */
function SizeField({
  label,
  bytes,
  onChange,
}: {
  label: string;
  bytes: number | null;
  onChange(bytes: number | null): void;
}) {
  const [draft, setDraft] = useState(() => toMb(bytes));
  const shown = toBytes(draft) === bytes ? draft : toMb(bytes);
  return (
    <label className="filter-field">
      {label}
      <input
        type="number"
        min={0}
        step="any"
        className="text-input"
        value={shown}
        onChange={(e) => {
          setDraft(e.target.value);
          onChange(toBytes(e.target.value));
        }}
      />
    </label>
  );
}

/** Art filters for media this layout can store. */
function artOptions(view: LibraryView): ArtFilter[] {
  const needs: Partial<Record<ArtFilter, 'cover' | 'screenshot' | 'title'>> = {
    'missing-cover': 'cover',
    'missing-screenshot': 'screenshot',
    'missing-title': 'title',
  };
  return ART_FILTERS.filter((a) => {
    const media = needs[a];
    return media === undefined || view.profile.media.includes(media);
  });
}

export function FilterPanel({
  view,
  facets,
}: {
  view: LibraryView;
  facets: Facets;
}) {
  const t = useT();
  const f = useFilterStore((s) => s.filters);
  const setArt = useFilterStore((s) => s.setArt);
  const toggleSystem = useFilterStore((s) => s.toggleSystem);
  const toggleExt = useFilterStore((s) => s.toggleExt);
  const toggleRegion = useFilterStore((s) => s.toggleRegion);
  const setDates = useFilterStore((s) => s.setDates);
  const setSizes = useFilterStore((s) => s.setSizes);
  const clear = useFilterStore((s) => s.clear);
  const exts = useMemo(
    () => [...new Set(view.entries.map((e) => e.ext))].sort(),
    [view.entries],
  );
  const regions = useMemo(() => {
    const present = new Set(view.entries.flatMap((e) => e.regions));
    return REGIONS.filter((r) => present.has(r));
  }, [view.entries]);

  return (
    <section
      id="filter-panel"
      className="filter-panel"
      aria-label={t('toolbar.filters')}
    >
      <fieldset>
        <legend>{t('filters.art')}</legend>
        {artOptions(view).map((a) => (
          <label key={a} className="filter-option">
            <input
              type="radio"
              name="art-filter"
              checked={f.art === a}
              onChange={() => setArt(a)}
            />
            <span>{t(`filters.artOptions.${a}`)}</span>
            <span className="filter-count">{facets.art[a]}</span>
          </label>
        ))}
      </fieldset>
      <fieldset>
        <legend>{t('filters.systems')}</legend>
        {view.systems
          .filter((s) => s.roms > 0)
          .map((s) => (
            <label key={s.index} className="filter-option">
              <input
                type="checkbox"
                checked={f.systems.includes(s.index)}
                onChange={() => toggleSystem(s.index)}
              />
              <span className="truncate">{s.name}</span>
              <span className="filter-count">
                {facets.systems.get(s.index) ?? 0}
              </span>
            </label>
          ))}
      </fieldset>
      <fieldset>
        <legend>{t('filters.exts')}</legend>
        {exts.map((x) => (
          <label key={x} className="filter-option">
            <input
              type="checkbox"
              checked={f.exts.includes(x)}
              onChange={() => toggleExt(x)}
            />
            <span>.{x}</span>
            <span className="filter-count">{facets.exts.get(x) ?? 0}</span>
          </label>
        ))}
      </fieldset>
      <fieldset>
        <legend>{t('filters.regions')}</legend>
        {regions.map((r) => (
          <label key={r} className="filter-option">
            <input
              type="checkbox"
              checked={f.regions.includes(r)}
              onChange={() => toggleRegion(r)}
            />
            <span>{t(`regions.${r}`)}</span>
            <span className="filter-count">{facets.regions.get(r) ?? 0}</span>
          </label>
        ))}
      </fieldset>
      <fieldset>
        <legend>{t('filters.date')}</legend>
        <label className="filter-field">
          {t('filters.from')}
          <input
            type="date"
            className="text-input"
            value={toDateInput(f.modifiedFrom)}
            onChange={(e) => setDates(dayStart(e.target.value), f.modifiedTo)}
          />
        </label>
        <label className="filter-field">
          {t('filters.to')}
          <input
            type="date"
            className="text-input"
            value={toDateInput(f.modifiedTo)}
            onChange={(e) => setDates(f.modifiedFrom, dayEnd(e.target.value))}
          />
        </label>
      </fieldset>
      <fieldset>
        <legend>{t('filters.size')}</legend>
        <SizeField
          label={t('filters.min')}
          bytes={f.sizeMin}
          onChange={(b) => setSizes(b, f.sizeMax)}
        />
        <SizeField
          label={t('filters.max')}
          bytes={f.sizeMax}
          onChange={(b) => setSizes(f.sizeMin, b)}
        />
      </fieldset>
      <button type="button" className="ghost-button" onClick={clear}>
        {t('filters.clear')}
      </button>
    </section>
  );
}
