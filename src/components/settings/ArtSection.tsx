import { useEffect, useState } from 'react';

import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group';
import { NativeSelect } from '@/components/ui/select';
import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import {
  MEDIA,
  type OutputFormat,
  type OverwriteSetting,
  type ProfileInfo,
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

const legendClass = 'mb-1.5 p-0 text-ui font-medium text-cr-text';

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
  const errorId = `max-size-${id.replace(/[^a-zA-Z0-9]+/g, '-')}-error`;
  const placeholder = slot?.max_width
    ? `${slot.max_width}`
    : t('settings.art.noLimit');
  return (
    <tr>
      <th scope="row">{info.profile.name}</th>
      {(['width', 'height'] as const).map((dim) => (
        <td key={dim}>
          <Input
            name={`max-${dim}-${id}`}
            size="sm"
            mono
            className="max-w-28"
            inputMode="numeric"
            aria-invalid={error ? true : undefined}
            aria-describedby={error ? errorId : undefined}
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
          <span id={errorId} role="alert" className={errorText}>
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
      <fieldset
        className="m-0 grid gap-2 border-0 p-0"
        aria-describedby={mediaError ? 'art-media-error' : undefined}
      >
        <legend className={legendClass}>{t('settings.art.media')}</legend>
        {MEDIA.map((m) => {
          const cid = settingId(`art.media.${m}`);
          return (
            <div key={m} className="flex items-center gap-2.5">
              <Checkbox
                id={cid}
                name={`media-${m}`}
                checked={art.media.includes(m)}
                aria-invalid={mediaError ? true : undefined}
                onCheckedChange={(on) =>
                  void update({
                    art: {
                      media:
                        on === true
                          ? [...art.media, m]
                          : art.media.filter((x) => x !== m),
                    },
                  })
                }
              />
              <Label htmlFor={cid} className="font-normal">
                {t(`art.media.${m}`)}
              </Label>
            </div>
          );
        })}
        {mediaError && (
          <p id="art-media-error" role="alert" className={errorText}>
            {mediaError}
          </p>
        )}
      </fieldset>
      <Field
        id={settingId('art.single_slot_media')}
        label={t('settings.art.singleSlot')}
        hint={t('settings.art.singleSlotHint')}
      >
        <NativeSelect
          name="single-slot"
          className="max-w-xs"
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
        </NativeSelect>
      </Field>
      <Field
        id={settingId('art.output_format')}
        label={t('settings.art.format')}
        hint={t('settings.art.formatHint')}
      >
        <NativeSelect
          name="output-format"
          className="max-w-xs"
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
        </NativeSelect>
      </Field>
      <SwitchSetting
        id={settingId('art.keep_aspect')}
        name="keep-aspect"
        label={t('settings.art.keepAspect')}
        checked={art.keep_aspect}
        onCheckedChange={(on) => void update({ art: { keep_aspect: on } })}
      />
      <NumberSetting
        path="art.jpeg_quality"
        label={t('settings.art.jpegQuality')}
        value={art.jpeg_quality}
        toPatch={(v) => ({ art: { jpeg_quality: v } })}
      />
      <fieldset className="m-0 border-0 p-0">
        <legend id="art-overwrite-legend" className={legendClass}>
          {t('settings.art.overwrite')}
        </legend>
        <RadioGroup
          name="overwrite"
          aria-labelledby="art-overwrite-legend"
          value={art.overwrite}
          onValueChange={(o) =>
            void update({ art: { overwrite: o as OverwriteSetting } })
          }
        >
          {(
            [
              'never',
              'ask',
              'backup',
            ] as const satisfies readonly OverwriteSetting[]
          ).map((o) => {
            const rid = settingId(`art.overwrite.${o}`);
            return (
              <div key={o} className="flex items-center gap-2.5">
                <RadioGroupItem id={rid} value={o} />
                <Label htmlFor={rid} className="font-normal">
                  {t(`settings.art.overwrites.${o}`)}
                </Label>
              </div>
            );
          })}
        </RadioGroup>
      </fieldset>
      <h2 className={sectionTitle}>{t('settings.art.maxSize')}</h2>
      <p className={hintText}>{t('settings.art.maxSizeHint')}</p>
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
