import { useContext, useId, useMemo, useState, type ReactNode } from 'react';

import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Field, FieldControl, FieldLabel } from '@/components/ui/field';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group';
import { SystemIcon } from '@/components/system/SystemIcon';
import { LocaleContext } from '@/i18n/core';

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
  name,
  label,
  bytes,
  onChange,
}: {
  name: string;
  label: string;
  bytes: number | null;
  onChange(bytes: number | null): void;
}) {
  const [draft, setDraft] = useState(() => toMb(bytes));
  const shown = toBytes(draft) === bytes ? draft : toMb(bytes);
  return (
    <Field>
      <FieldLabel>{label}</FieldLabel>
      <FieldControl>
        <Input
          type="number"
          name={name}
          min={0}
          step="any"
          size="sm"
          mono
          value={shown}
          onChange={(e) => {
            setDraft(e.target.value);
            onChange(toBytes(e.target.value));
          }}
        />
      </FieldControl>
    </Field>
  );
}

/**
 * One fieldset of the panel: the legend names the group and sits on the
 * shelf rule (brand §2: 1px rule, 3px Buckram cap on the left), pinned while
 * a long list (systems, extensions) scrolls under it.
 */
function Group({
  legend,
  legendId,
  children,
}: {
  legend: string;
  legendId?: string;
  children: ReactNode;
}) {
  return (
    <fieldset className="m-0 grid max-h-56 min-w-48 content-start gap-1.5 overflow-auto border-0 p-0 pr-1">
      <legend
        id={legendId}
        className="sticky top-0 z-[1] float-left mb-0.5 w-full border-b border-cr-border bg-cr-surface pb-1 pl-2 text-caption font-semibold text-cr-text before:absolute before:bottom-1 before:left-0 before:top-0 before:w-[3px] before:rounded-r-sm before:bg-primary"
      >
        {legend}
      </legend>
      {children}
    </fieldset>
  );
}

/** A choice row: control, an optional icon, its label, then the count it
 *  would show. */
function Option({
  id,
  control,
  icon,
  label,
  count,
}: {
  id: string;
  control: ReactNode;
  icon?: ReactNode;
  label: ReactNode;
  count: string;
}) {
  return (
    <div className="flex min-w-0 items-center gap-2">
      {control}
      <Label
        htmlFor={id}
        className="flex min-w-0 flex-1 cursor-pointer items-center gap-2 font-normal"
      >
        {icon}
        <span className="truncate">{label}</span>
        <Badge mono className="ml-auto">
          {count}
        </Badge>
      </Label>
    </div>
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
  const uid = useId();
  const locale = useContext(LocaleContext);
  const n = useMemo(() => new Intl.NumberFormat(locale), [locale]);
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

  const id = (kind: string, value: string | number) =>
    `${uid}-${kind}-${value}`;

  return (
    <section
      id="filter-panel"
      className="flex max-h-[40vh] flex-wrap items-start gap-4 overflow-auto border-b border-cr-border bg-cr-surface px-4 py-3"
      aria-label={t('toolbar.filters')}
    >
      {/* A form so the Radix radios and checkboxes carry named native
          inputs (autofill, devtools); nothing is ever submitted. */}
      <form
        className="contents"
        onSubmit={(e) => e.preventDefault()}
        noValidate
      >
        <Group legend={t('filters.art')} legendId={`${uid}-art`}>
          <RadioGroup
            name="art-filter"
            aria-labelledby={`${uid}-art`}
            value={f.art}
            onValueChange={(v) => setArt(v as ArtFilter)}
            className="gap-1.5"
          >
            {artOptions(view).map((a) => (
              <Option
                key={a}
                id={id('art', a)}
                control={<RadioGroupItem id={id('art', a)} value={a} />}
                label={t(`filters.artOptions.${a}`)}
                count={n.format(facets.art[a])}
              />
            ))}
          </RadioGroup>
        </Group>
        <Group legend={t('filters.systems')}>
          {view.systems
            .filter((s) => s.roms > 0)
            .map((s) => (
              <Option
                key={s.index}
                id={id('system', s.index)}
                control={
                  <Checkbox
                    id={id('system', s.index)}
                    name="system"
                    value={s.folder}
                    checked={f.systems.includes(s.index)}
                    onCheckedChange={() => toggleSystem(s.index)}
                  />
                }
                icon={<SystemIcon systemId={s.system_id} size={16} />}
                label={s.name}
                count={n.format(facets.systems.get(s.index) ?? 0)}
              />
            ))}
        </Group>
        <Group legend={t('filters.exts')}>
          {exts.map((x) => (
            <Option
              key={x}
              id={id('ext', x)}
              control={
                <Checkbox
                  id={id('ext', x)}
                  name="extension"
                  value={x}
                  checked={f.exts.includes(x)}
                  onCheckedChange={() => toggleExt(x)}
                />
              }
              label={`.${x}`}
              count={n.format(facets.exts.get(x) ?? 0)}
            />
          ))}
        </Group>
        <Group legend={t('filters.regions')}>
          {regions.map((r) => (
            <Option
              key={r}
              id={id('region', r)}
              control={
                <Checkbox
                  id={id('region', r)}
                  name="region"
                  value={r}
                  checked={f.regions.includes(r)}
                  onCheckedChange={() => toggleRegion(r)}
                />
              }
              label={t(`regions.${r}`)}
              count={n.format(facets.regions.get(r) ?? 0)}
            />
          ))}
        </Group>
        <Group legend={t('filters.date')}>
          <Field>
            <FieldLabel>{t('filters.from')}</FieldLabel>
            <FieldControl>
              <Input
                type="date"
                name="modified-from"
                size="sm"
                value={toDateInput(f.modifiedFrom)}
                onChange={(e) =>
                  setDates(dayStart(e.target.value), f.modifiedTo)
                }
              />
            </FieldControl>
          </Field>
          <Field>
            <FieldLabel>{t('filters.to')}</FieldLabel>
            <FieldControl>
              <Input
                type="date"
                name="modified-to"
                size="sm"
                value={toDateInput(f.modifiedTo)}
                onChange={(e) =>
                  setDates(f.modifiedFrom, dayEnd(e.target.value))
                }
              />
            </FieldControl>
          </Field>
        </Group>
        <Group legend={t('filters.size')}>
          <SizeField
            name="size-min"
            label={t('filters.min')}
            bytes={f.sizeMin}
            onChange={(b) => setSizes(b, f.sizeMax)}
          />
          <SizeField
            name="size-max"
            label={t('filters.max')}
            bytes={f.sizeMax}
            onChange={(b) => setSizes(f.sizeMin, b)}
          />
        </Group>
      </form>
      <Button variant="outline" onClick={clear}>
        {t('filters.clear')}
      </Button>
    </section>
  );
}
