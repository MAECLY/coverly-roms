import { useEffect, useState } from 'react';

import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import {
  MEDIA,
  type OutputFormat,
  type OverwriteSetting,
  type ProfileInfo,
} from '@/ipc/types';
import { useSettingsStore } from '@/stores/settingsStore';

import { Field, NumberSetting, settingId, useFieldError } from './Field';

/** A positive whole number of pixels, or null. */
function pixels(text: string): number | null {
  return /^\d{1,6}$/.test(text) && Number(text) > 0 ? Number(text) : null;
}

function SizeRow({ info }: { info: ProfileInfo }) {
  const t = useT();
  const id = info.profile.id;
  const art = useSettingsStore((s) => s.settings!.art);
  const update = useSettingsStore((s) => s.update);
  const cur = art.max_size[id];
  const slot = info.profile.slots.cover ?? Object.values(info.profile.slots)[0];
  // Typed text over the stored size: a reset or another save shows through.
  const [dw, setW] = useState<string | null>(null);
  const [dh, setH] = useState<string | null>(null);
  const [local, setLocal] = useState<string | null>(null);
  const w = dw ?? (cur ? String(cur.width) : '');
  const h = dh ?? (cur ? String(cur.height) : '');
  const errW = useFieldError(`art.max_size.${id}.width`);
  const errH = useFieldError(`art.max_size.${id}.height`);
  async function commit() {
    if (dw === null && dh === null) return;
    const wt = w.trim();
    const ht = h.trim();
    // Half-typed (one of the two empty): wait for the other value.
    if ((wt === '') !== (ht === '')) return;
    let errors;
    if (wt === '') {
      setLocal(null);
      // A merge patch merges maps key by key: removing a size needs `null`.
      errors = cur ? await update({ art: { max_size: { [id]: null } } }) : [];
    } else {
      const width = pixels(wt);
      const height = pixels(ht);
      if (width === null || height === null) {
        setLocal(t('errors.field.invalid'));
        return;
      }
      setLocal(null);
      errors = await update({ art: { max_size: { [id]: { width, height } } } });
    }
    if (errors.length === 0) {
      setW(null);
      setH(null);
    }
  }
  const error = local ?? errW ?? errH;
  const placeholder = slot?.max_width
    ? `${slot.max_width}`
    : t('settings.art.noLimit');
  return (
    <tr>
      <th scope="row">{info.profile.name}</th>
      {(['width', 'height'] as const).map((dim) => (
        <td key={dim}>
          <input
            name={`max-${dim}-${id}`}
            className="text-input setting-number"
            inputMode="numeric"
            aria-invalid={error ? true : undefined}
            aria-label={t(
              `settings.art.max${dim === 'width' ? 'Width' : 'Height'}`,
              { profile: info.profile.name },
            )}
            placeholder={
              dim === 'width'
                ? placeholder
                : slot?.max_height
                  ? `${slot.max_height}`
                  : t('settings.art.noLimit')
            }
            value={dim === 'width' ? w : h}
            onChange={(e) => (dim === 'width' ? setW : setH)(e.target.value)}
            onBlur={() => void commit()}
            onKeyDown={(e) => e.key === 'Enter' && void commit()}
          />
        </td>
      ))}
      <td>
        {error && (
          <span role="alert" className="field-error">
            {error}
          </span>
        )}
      </td>
    </tr>
  );
}

export function ArtSection() {
  const t = useT();
  const art = useSettingsStore((s) => s.settings!.art);
  const update = useSettingsStore((s) => s.update);
  const mediaError = useFieldError('art.media');
  const [profiles, setProfiles] = useState<ProfileInfo[]>([]);
  useEffect(() => {
    let alive = true;
    void getBackend()
      .then((b) => b.listProfiles())
      .then(
        (p) => alive && setProfiles(p),
        () => undefined,
      );
    return () => {
      alive = false;
    };
  }, []);
  return (
    <div className="settings-group">
      <fieldset className="setting">
        <legend className="setting-label">{t('settings.art.media')}</legend>
        {MEDIA.map((m) => (
          <label key={m} className="checkbox-row">
            <input
              type="checkbox"
              name={`media-${m}`}
              checked={art.media.includes(m)}
              onChange={(e) =>
                void update({
                  art: {
                    media: e.target.checked
                      ? [...art.media, m]
                      : art.media.filter((x) => x !== m),
                  },
                })
              }
            />
            {t(`art.media.${m}`)}
          </label>
        ))}
        {mediaError && (
          <p role="alert" className="field-error">
            {mediaError}
          </p>
        )}
      </fieldset>
      <Field
        id={settingId('art.single_slot_media')}
        label={t('settings.art.singleSlot')}
        hint={t('settings.art.singleSlotHint')}
      >
        <select
          id={settingId('art.single_slot_media')}
          name="single-slot"
          className="select"
          value={art.single_slot_media}
          onChange={(e) =>
            void update({
              art: {
                single_slot_media: e.target.value as (typeof MEDIA)[number],
              },
            })
          }
        >
          {MEDIA.map((m) => (
            <option key={m} value={m}>
              {t(`art.media.${m}`)}
            </option>
          ))}
        </select>
      </Field>
      <Field
        id={settingId('art.output_format')}
        label={t('settings.art.format')}
        hint={t('settings.art.formatHint')}
      >
        <select
          id={settingId('art.output_format')}
          name="output-format"
          className="select"
          value={art.output_format}
          onChange={(e) =>
            void update({
              art: { output_format: e.target.value as OutputFormat },
            })
          }
        >
          {(['profile', 'png', 'jpg'] as const).map((f) => (
            <option key={f} value={f}>
              {t(`settings.art.formats.${f}`)}
            </option>
          ))}
        </select>
      </Field>
      <label className="checkbox-row">
        <input
          type="checkbox"
          name="keep-aspect"
          checked={art.keep_aspect}
          onChange={(e) =>
            void update({ art: { keep_aspect: e.target.checked } })
          }
        />
        {t('settings.art.keepAspect')}
      </label>
      <NumberSetting
        path="art.jpeg_quality"
        label={t('settings.art.jpegQuality')}
        value={art.jpeg_quality}
        toPatch={(v) => ({ art: { jpeg_quality: v } })}
      />
      <fieldset className="setting">
        <legend className="setting-label">{t('settings.art.overwrite')}</legend>
        {(
          [
            'never',
            'ask',
            'backup',
          ] as const satisfies readonly OverwriteSetting[]
        ).map((o) => (
          <label key={o} className="radio-row">
            <input
              type="radio"
              name="overwrite"
              checked={art.overwrite === o}
              onChange={() => void update({ art: { overwrite: o } })}
            />
            {t(`settings.art.overwrites.${o}`)}
          </label>
        ))}
      </fieldset>
      <h2 className="section-title">{t('settings.art.maxSize')}</h2>
      <p className="hint">{t('settings.art.maxSizeHint')}</p>
      <table className="settings-table">
        <thead>
          <tr>
            <th scope="col">{t('settings.art.profile')}</th>
            <th scope="col">{t('settings.art.width')}</th>
            <th scope="col">{t('settings.art.height')}</th>
            <th />
          </tr>
        </thead>
        <tbody>
          {profiles.map((p) => (
            <SizeRow key={p.profile.id} info={p} />
          ))}
        </tbody>
      </table>
    </div>
  );
}
