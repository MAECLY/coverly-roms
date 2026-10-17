import { ChevronDown, ChevronUp, X } from 'lucide-react';
import { useState } from 'react';

import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group';
import {
  NativeSelect,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { SimpleTooltip } from '@/components/ui/tooltip';
import { useT } from '@/i18n/useT';
import {
  REGIONS,
  STRATEGIES,
  type MatchMethod,
  type RegionPref,
  type StrategyKind,
} from '@/ipc/types';
import { useSettingsStore } from '@/stores/settingsStore';

import {
  errorText,
  Field,
  hintText,
  NumberSetting,
  sectionTitle,
  settingId,
  SwitchSetting,
  useFieldError,
} from './Field';
import { ReorderAnnouncer, useReorderAnnouncer } from './ReorderAnnouncer';

const METHODS: readonly MatchMethod[] = [
  'hash-then-name',
  'hash-only',
  'name-only',
];

export function MatchingSection() {
  const t = useT();
  const m = useSettingsStore((s) => s.settings!.matching);
  const update = useSettingsStore((s) => s.update);
  const regionsError = useFieldError('matching.regions');
  const langError = useFieldError('matching.languages');
  const [langs, setLangs] = useState<string | null>(null);
  const announcer = useReorderAnnouncer();
  const label = (r: RegionPref) =>
    r === 'rom' ? t('fill.regionRom') : t(`regions.${r}`);
  const setRegions = (next: RegionPref[]) =>
    void update({ matching: { regions: next } });
  const move = (i: number, d: -1 | 1) => {
    const next = [...m.regions];
    const [x] = next.splice(i, 1);
    next.splice(i + d, 0, x!);
    setRegions(next);
    announcer.moved(label(x!), i + d + 1, next.length);
  };
  const missing = (['rom', ...REGIONS] as RegionPref[]).filter(
    (r) => !m.regions.includes(r),
  );
  const langId = settingId('matching.languages');
  return (
    <div className="settings-group">
      <fieldset className="m-0 border-0 p-0">
        <legend
          id="matching-strategy-legend"
          className="mb-1.5 p-0 text-ui font-medium text-cr-text"
        >
          {t('settings.matching.strategy')}
        </legend>
        <RadioGroup
          name="strategy"
          aria-labelledby="matching-strategy-legend"
          className="gap-3"
          value={m.strategy}
          onValueChange={(s) =>
            void update({
              matching: { strategy: s as StrategyKind },
            })
          }
        >
          {STRATEGIES.map((s) => {
            const rid = settingId(`matching.strategy.${s}`);
            return (
              <div
                key={s}
                className="grid grid-cols-[auto_1fr] items-start gap-x-2.5 gap-y-0.5"
              >
                <RadioGroupItem
                  id={rid}
                  value={s}
                  className="mt-0.5"
                  aria-describedby={`${rid}-hint`}
                />
                <Label htmlFor={rid} className="font-normal">
                  {t(`fill.strategy.${s}`)}
                </Label>
                <p id={`${rid}-hint`} className={`${hintText} col-start-2`}>
                  {t(`fill.strategyHint.${s}`)}
                </p>
              </div>
            );
          })}
        </RadioGroup>
      </fieldset>
      <h2 className={sectionTitle} id="settings-regions">
        {t('settings.matching.regions')}
      </h2>
      <ol
        className="m-0 grid list-decimal gap-1 pl-6 marker:text-caption marker:text-cr-text-secondary"
        aria-labelledby="settings-regions"
        aria-describedby={regionsError ? 'settings-regions-error' : undefined}
      >
        {m.regions.map((r, i) => {
          const up = t('fill.moveUp', { region: label(r) });
          const down = t('fill.moveDown', { region: label(r) });
          const remove = t('settings.matching.remove', { region: label(r) });
          return (
            <li key={r} className="pl-1">
              <div className="flex items-center gap-1">
                <span className="min-w-0 flex-1 truncate text-ui">
                  {label(r)}
                </span>
                <SimpleTooltip content={up}>
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    disabled={i === 0}
                    aria-label={up}
                    onClick={() => move(i, -1)}
                  >
                    <ChevronUp aria-hidden />
                  </Button>
                </SimpleTooltip>
                <SimpleTooltip content={down}>
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    disabled={i === m.regions.length - 1}
                    aria-label={down}
                    onClick={() => move(i, 1)}
                  >
                    <ChevronDown aria-hidden />
                  </Button>
                </SimpleTooltip>
                <SimpleTooltip content={remove}>
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    aria-label={remove}
                    onClick={() => setRegions(m.regions.filter((x) => x !== r))}
                  >
                    <X aria-hidden />
                  </Button>
                </SimpleTooltip>
              </div>
            </li>
          );
        })}
      </ol>
      <ReorderAnnouncer message={announcer.message} />
      {missing.length > 0 && (
        // Remounted with the list so it always shows its placeholder.
        <Select
          key={m.regions.join()}
          name="add-region"
          onValueChange={(v) => setRegions([...m.regions, v as RegionPref])}
        >
          <SelectTrigger
            className="w-auto min-w-48 max-w-xs"
            aria-label={t('settings.matching.addRegion')}
          >
            <SelectValue placeholder={t('settings.matching.addRegion')} />
          </SelectTrigger>
          <SelectContent>
            {missing.map((r) => (
              <SelectItem key={r} value={r}>
                {label(r)}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      )}
      {regionsError && (
        <p id="settings-regions-error" role="alert" className={errorText}>
          {regionsError}
        </p>
      )}
      <Field
        id={langId}
        label={t('settings.matching.languages')}
        hint={t('settings.matching.languagesHint')}
        error={langError}
      >
        <Input
          name={langId}
          className="max-w-xs"
          value={langs ?? m.languages.join(', ')}
          onChange={(e) => setLangs(e.target.value)}
          onBlur={async () => {
            if (langs === null) return;
            const list = langs
              .split(',')
              .map((x) => x.trim().toLowerCase())
              .filter(Boolean);
            if ((await update({ matching: { languages: list } })).length === 0)
              setLangs(null);
          }}
        />
      </Field>
      <NumberSetting
        path="matching.auto_threshold"
        label={t('settings.matching.auto')}
        value={m.auto_threshold}
        scale={100}
        toPatch={(v) => ({ matching: { auto_threshold: v } })}
      />
      <NumberSetting
        path="matching.review_threshold"
        label={t('settings.matching.review')}
        value={m.review_threshold}
        scale={100}
        toPatch={(v) => ({ matching: { review_threshold: v } })}
      />
      <SwitchSetting
        id={settingId('matching.fuzzy')}
        name="fuzzy"
        label={t('settings.matching.fuzzy')}
        checked={m.fuzzy}
        onCheckedChange={(on) => void update({ matching: { fuzzy: on } })}
      />
      <SwitchSetting
        id={settingId('matching.arcade_translation')}
        name="arcade"
        label={t('settings.matching.arcade')}
        checked={m.arcade_translation}
        onCheckedChange={(on) =>
          void update({ matching: { arcade_translation: on } })
        }
      />
      <Field
        id={settingId('matching.match_method')}
        label={t('settings.matching.method')}
      >
        <NativeSelect
          name="match-method"
          className="max-w-xs"
          value={m.match_method}
          onChange={(e) =>
            void update({
              matching: { match_method: e.target.value as MatchMethod },
            })
          }
        >
          {METHODS.map((x) => (
            <option key={x} value={x}>
              {t(`settings.matching.methods.${x}`)}
            </option>
          ))}
        </NativeSelect>
      </Field>
      <NumberSetting
        path="matching.max_hash_mb"
        label={t('settings.matching.maxHash')}
        value={m.max_hash_mb}
        toPatch={(v) => ({ matching: { max_hash_mb: v } })}
      />
    </div>
  );
}
