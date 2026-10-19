import {
  ArrowDown,
  ArrowDownWideNarrow,
  ArrowUp,
  Settings2,
} from 'lucide-react';
import { useId, type ReactNode } from 'react';

import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Label } from '@/components/ui/label';
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from '@/components/ui/popover';
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group';
import { NativeSelect } from '@/components/ui/select';
import { Slider } from '@/components/ui/slider';
import { Switch } from '@/components/ui/switch';
import { Toggle } from '@/components/ui/toggle';
import { useT } from '@/i18n/useT';
import type { Media } from '@/ipc/types';
import { COLUMN_DEFS, moveColumn, toggleColumn } from '@/lib/detailsColumns';
import type { SortKey } from '@/lib/sort';
import {
  COVER_SIZE_IDS,
  DENSITY_IDS,
  DETAILS_COLUMN_IDS,
  GROUP_BY_IDS,
  INSPECTOR_VIEWS,
  type Density,
  type GroupBy,
  type ViewId,
} from '@/lib/viewPrefs';
import { useViewStore } from '@/stores/viewStore';

import { Tip } from './Tip';

const SIZED: readonly ViewId[] = ['covers', 'shelves'];
const ART_CHOICE: readonly ViewId[] = ['covers', 'shelves'];
const SORT_KEYS: readonly SortKey[] = [
  'library',
  'name',
  'system',
  'size',
  'mtime',
];
const SORT_LABEL: Record<SortKey, string> = {
  library: 'layouts.sortLibrary',
  name: 'columns.name',
  system: 'columns.system',
  size: 'columns.size',
  mtime: 'columns.modified',
};
const ART_CHOICES: readonly { media: Media; key: string }[] = [
  { media: 'cover', key: 'layouts.artCover' },
  { media: 'title', key: 'layouts.artTitle' },
  { media: 'screenshot', key: 'layouts.artScreenshot' },
];
const DENSITY_LABEL: Record<Density, string> = {
  comfortable: 'layouts.densityComfortable',
  compact: 'layouts.densityCompact',
};

/** What a view offers (spec §B.6): only the controls that apply to it. */
function controlsFor(view: ViewId) {
  return {
    size: SIZED.includes(view),
    density: view === 'details',
    art: ART_CHOICE.includes(view),
    groupBy: view !== 'columns',
    sort: view !== 'details',
    columns: view === 'details',
    inspector: INSPECTOR_VIEWS.includes(view),
  };
}

/** One labelled block of the popover; the label names the control. */
function Section({
  label,
  labelId,
  aside,
  children,
}: {
  label: string;
  labelId: string;
  aside?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="view-options__section">
      <div className="flex items-baseline justify-between gap-2">
        <span id={labelId} className="text-caption text-cr-text-secondary">
          {label}
        </span>
        {aside}
      </div>
      {children}
    </section>
  );
}

/**
 * The View options popover next to the view switcher (spec §B.6). It shows
 * only what applies to the current view: size and art in Covers and
 * Shelves, density and columns in Details, group and sort where they mean
 * something, and the docked inspector where the view has no preview pane.
 */
export function ViewOptions() {
  const t = useT();
  const prefs = useViewStore((s) => s.prefs);
  const sort = useViewStore((s) => s.sort);
  const setPrefs = useViewStore((s) => s.setPrefs);
  const setSort = useViewStore((s) => s.setSort);
  const id = useId().replace(/:/g, '');
  const ids = {
    size: `${id}-size`,
    density: `${id}-density`,
    art: `${id}-art`,
    group: `${id}-group`,
    sort: `${id}-sort`,
    columns: `${id}-columns`,
    inspector: `${id}-inspector`,
  };
  const show = controlsFor(prefs.view);
  const shelves = prefs.view === 'shelves';
  // Shelves are always grouped: no grouping reads as System (spec §B.5).
  const groupValue: GroupBy =
    shelves && prefs.groupBy === 'none' ? 'system' : prefs.groupBy;
  const groupChoices = shelves
    ? GROUP_BY_IDS.filter((g) => g !== 'none')
    : GROUP_BY_IDS;
  const sizeIndex = COVER_SIZE_IDS.indexOf(prefs.coverSize);
  const shown = new Set(prefs.columns.map((c) => c.id));
  const hidden = DETAILS_COLUMN_IDS.filter((c) => !shown.has(c));

  return (
    <Popover>
      <Tip content={t('layouts.options')}>
        <PopoverTrigger asChild>
          <Button
            variant="ghost"
            size="icon"
            className="text-cr-text-secondary"
            aria-label={t('layouts.options')}
          >
            <Settings2 aria-hidden />
          </Button>
        </PopoverTrigger>
      </Tip>
      <PopoverContent
        align="end"
        aria-label={t('layouts.options')}
        className="view-options w-[17rem] p-0"
      >
        {show.size && (
          <Section
            label={t('layouts.size')}
            labelId={ids.size}
            aside={
              <span aria-hidden className="text-caption text-cr-text">
                {t(`layouts.sizeStops.${prefs.coverSize}`)}
              </span>
            }
          >
            <Slider
              min={0}
              max={COVER_SIZE_IDS.length - 1}
              step={1}
              value={[sizeIndex]}
              thumbLabel={t('layouts.size')}
              thumbValueText={t(`layouts.sizeStops.${prefs.coverSize}`)}
              className="py-1.5"
              onValueChange={([i]) => {
                const next = COVER_SIZE_IDS[i ?? sizeIndex];
                if (next) setPrefs({ coverSize: next });
              }}
            />
          </Section>
        )}

        {show.density && (
          <Section label={t('layouts.density')} labelId={ids.density}>
            <RadioGroup
              aria-labelledby={ids.density}
              orientation="horizontal"
              value={prefs.density}
              onValueChange={(v) => setPrefs({ density: v as Density })}
            >
              {DENSITY_IDS.map((d) => (
                <div key={d} className="flex items-center gap-2">
                  <RadioGroupItem id={`${ids.density}-${d}`} value={d} />
                  <Label
                    htmlFor={`${ids.density}-${d}`}
                    className="font-normal"
                  >
                    {t(DENSITY_LABEL[d])}
                  </Label>
                </div>
              ))}
            </RadioGroup>
          </Section>
        )}

        {show.art && (
          <Section label={t('layouts.art')} labelId={ids.art}>
            <RadioGroup
              aria-labelledby={ids.art}
              value={prefs.art}
              onValueChange={(v) => setPrefs({ art: v as Media })}
            >
              {ART_CHOICES.map(({ media, key }) => (
                <div key={media} className="flex items-center gap-2">
                  <RadioGroupItem id={`${ids.art}-${media}`} value={media} />
                  <Label
                    htmlFor={`${ids.art}-${media}`}
                    className="font-normal"
                  >
                    {t(key)}
                  </Label>
                </div>
              ))}
            </RadioGroup>
          </Section>
        )}

        {show.groupBy && (
          <Section label={t('layouts.groupBy')} labelId={ids.group}>
            <NativeSelect
              size="sm"
              aria-labelledby={ids.group}
              value={groupValue}
              onChange={(e) => setPrefs({ groupBy: e.target.value as GroupBy })}
            >
              {groupChoices.map((g) => (
                <option key={g} value={g}>
                  {t(`layouts.group.${g}`)}
                </option>
              ))}
            </NativeSelect>
            {shelves && (
              <Button
                variant="ghost"
                size="sm"
                className="justify-self-start"
                aria-pressed={prefs.groupBy === 'art'}
                onClick={() => setPrefs({ groupBy: 'art' })}
              >
                <ArrowDownWideNarrow aria-hidden />
                {t('layouts.missingArtFirst')}
              </Button>
            )}
          </Section>
        )}

        {show.sort && (
          <Section label={t('layouts.sortBy')} labelId={ids.sort}>
            <div className="flex items-center gap-1.5">
              <NativeSelect
                size="sm"
                aria-labelledby={ids.sort}
                value={sort.key}
                onChange={(e) =>
                  setSort({ key: e.target.value as SortKey, dir: sort.dir })
                }
              >
                {SORT_KEYS.map((k) => (
                  <option key={k} value={k}>
                    {t(SORT_LABEL[k])}
                  </option>
                ))}
              </NativeSelect>
              <Toggle
                variant="outline"
                size="sm"
                aria-label={t('layouts.sortDescending')}
                pressed={sort.dir === 'desc'}
                onPressedChange={(on) =>
                  setSort({ key: sort.key, dir: on ? 'desc' : 'asc' })
                }
              >
                {sort.dir === 'desc' ? (
                  <ArrowDown aria-hidden />
                ) : (
                  <ArrowUp aria-hidden />
                )}
              </Toggle>
            </div>
          </Section>
        )}

        {show.columns && (
          <Section label={t('layouts.columnsPicker')} labelId={ids.columns}>
            <ul
              role="group"
              aria-labelledby={ids.columns}
              className="view-options__columns"
            >
              {prefs.columns.map((c, i) => {
                const name = t(COLUMN_DEFS[c.id].labelKey);
                const cid = `${ids.columns}-${c.id}`;
                return (
                  <li key={c.id} className="view-options__column">
                    <Checkbox
                      id={cid}
                      checked
                      disabled={c.id === 'name'}
                      onCheckedChange={() =>
                        setPrefs({
                          columns: toggleColumn(prefs.columns, c.id),
                        })
                      }
                    />
                    <Label
                      htmlFor={cid}
                      className="flex-1 truncate font-normal"
                    >
                      {name}
                    </Label>
                    <Button
                      variant="ghost"
                      size="icon-sm"
                      aria-label={t('layouts.columnMoveUp', { column: name })}
                      disabled={i === 0}
                      onClick={() =>
                        setPrefs({
                          columns: moveColumn(prefs.columns, c.id, -1),
                        })
                      }
                    >
                      <ArrowUp aria-hidden />
                    </Button>
                    <Button
                      variant="ghost"
                      size="icon-sm"
                      aria-label={t('layouts.columnMoveDown', { column: name })}
                      disabled={i === prefs.columns.length - 1}
                      onClick={() =>
                        setPrefs({
                          columns: moveColumn(prefs.columns, c.id, 1),
                        })
                      }
                    >
                      <ArrowDown aria-hidden />
                    </Button>
                  </li>
                );
              })}
              {hidden.map((c) => {
                const cid = `${ids.columns}-${c}`;
                return (
                  <li key={c} className="view-options__column">
                    <Checkbox
                      id={cid}
                      checked={false}
                      onCheckedChange={() =>
                        setPrefs({ columns: toggleColumn(prefs.columns, c) })
                      }
                    />
                    <Label
                      htmlFor={cid}
                      className="flex-1 truncate font-normal text-cr-text-secondary"
                    >
                      {t(COLUMN_DEFS[c].labelKey)}
                    </Label>
                  </li>
                );
              })}
            </ul>
          </Section>
        )}

        {show.inspector && (
          <div className="view-options__section flex items-center justify-between gap-3">
            <Label htmlFor={ids.inspector} className="font-normal">
              {t('layouts.inspector')}
            </Label>
            <Switch
              id={ids.inspector}
              checked={prefs.inspector}
              onCheckedChange={(on) => setPrefs({ inspector: on })}
            />
          </div>
        )}
      </PopoverContent>
    </Popover>
  );
}
