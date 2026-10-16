import { useState } from 'react';

import { useT } from '@/i18n/useT';
import {
  REGIONS,
  STRATEGIES,
  type MatchMethod,
  type RegionPref,
} from '@/ipc/types';
import { useSettingsStore } from '@/stores/settingsStore';

import { Field, NumberSetting, settingId, useFieldError } from './Field';

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
  const label = (r: RegionPref) =>
    r === 'rom' ? t('fill.regionRom') : t(`regions.${r}`);
  const setRegions = (next: RegionPref[]) =>
    void update({ matching: { regions: next } });
  const move = (i: number, d: -1 | 1) => {
    const next = [...m.regions];
    const [x] = next.splice(i, 1);
    next.splice(i + d, 0, x!);
    setRegions(next);
  };
  const missing = (['rom', ...REGIONS] as RegionPref[]).filter(
    (r) => !m.regions.includes(r),
  );
  const langId = settingId('matching.languages');
  return (
    <div className="settings-group">
      <fieldset className="setting">
        <legend className="setting-label">
          {t('settings.matching.strategy')}
        </legend>
        {STRATEGIES.map((s) => (
          <div key={s}>
            <label className="radio-row">
              <input
                type="radio"
                name="strategy"
                checked={m.strategy === s}
                onChange={() => void update({ matching: { strategy: s } })}
              />
              {t(`fill.strategy.${s}`)}
            </label>
            <p className="hint">{t(`fill.strategyHint.${s}`)}</p>
          </div>
        ))}
      </fieldset>
      <h2 className="section-title" id="settings-regions">
        {t('settings.matching.regions')}
      </h2>
      <ol className="region-order" aria-labelledby="settings-regions">
        {m.regions.map((r, i) => (
          <li key={r}>
            <span>{label(r)}</span>
            <button
              type="button"
              className="icon-button"
              disabled={i === 0}
              aria-label={t('fill.moveUp', { region: label(r) })}
              onClick={() => move(i, -1)}
            >
              ↑
            </button>
            <button
              type="button"
              className="icon-button"
              disabled={i === m.regions.length - 1}
              aria-label={t('fill.moveDown', { region: label(r) })}
              onClick={() => move(i, 1)}
            >
              ↓
            </button>
            <button
              type="button"
              className="icon-button"
              aria-label={t('settings.matching.remove', { region: label(r) })}
              onClick={() => setRegions(m.regions.filter((x) => x !== r))}
            >
              ×
            </button>
          </li>
        ))}
      </ol>
      {missing.length > 0 && (
        <select
          name="add-region"
          className="select"
          aria-label={t('settings.matching.addRegion')}
          value=""
          onChange={(e) =>
            e.target.value &&
            setRegions([...m.regions, e.target.value as RegionPref])
          }
        >
          <option value="">{t('settings.matching.addRegion')}</option>
          {missing.map((r) => (
            <option key={r} value={r}>
              {label(r)}
            </option>
          ))}
        </select>
      )}
      {regionsError && (
        <p role="alert" className="field-error">
          {regionsError}
        </p>
      )}
      <Field
        id={langId}
        label={t('settings.matching.languages')}
        hint={t('settings.matching.languagesHint')}
        error={langError}
      >
        <input
          id={langId}
          name={langId}
          className="text-input"
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
      <label className="checkbox-row">
        <input
          type="checkbox"
          name="fuzzy"
          checked={m.fuzzy}
          onChange={(e) =>
            void update({ matching: { fuzzy: e.target.checked } })
          }
        />
        {t('settings.matching.fuzzy')}
      </label>
      <label className="checkbox-row">
        <input
          type="checkbox"
          name="arcade"
          checked={m.arcade_translation}
          onChange={(e) =>
            void update({ matching: { arcade_translation: e.target.checked } })
          }
        />
        {t('settings.matching.arcade')}
      </label>
      <Field
        id={settingId('matching.match_method')}
        label={t('settings.matching.method')}
      >
        <select
          id={settingId('matching.match_method')}
          name="match-method"
          className="select"
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
        </select>
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
