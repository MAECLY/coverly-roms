import { Fragment, useEffect, useRef, useState } from 'react';

import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import { errorKey, toBackendError, type BackendError } from '@/ipc/errors';
import {
  MEDIA,
  type FieldError,
  type Media,
  type PreviewExample,
  type ProfileFile,
  type ProfileInfo,
  type SlotFile,
} from '@/ipc/types';

import { fieldErrorText } from './Field';

// eslint-disable-next-line react-refresh/only-export-components -- the blank profile is part of the editor's interface (plan)
export const BLANK_PROFILE: ProfileFile = {
  id: 'my-layout',
  name: 'My layout',
  roms_root: 'ROMS',
  art_dirs: ['Imgs'],
  gamelist: false,
  slots: {
    cover: {
      template: '{system_dir}/{rel_dir}/Imgs/{stem}.png',
      format: 'png',
      max_width: null,
      max_height: null,
    },
  },
};

/** A size as typed: blank is "no limit", anything else a whole number. */
const SIZE = /^\d{1,9}$/;

const EMPTY_SLOT: SlotFile = {
  template: '{system_dir}/{rel_dir}/media/{stem}.png',
  format: 'png',
  max_width: null,
  max_height: null,
};

export function ProfileEditor({
  initial,
  replaces,
  onSaved,
  onCancel,
}: {
  initial: ProfileFile;
  replaces: string | null;
  onSaved(list: ProfileInfo[]): void;
  onCancel(): void;
}) {
  const t = useT();
  const [draft, setDraft] = useState<ProfileFile>(initial);
  const [example, setExample] = useState<PreviewExample>({
    system_folder: 'SFC',
    rel_dir: '',
    file_name: 'Zelda (USA).sfc',
  });
  const [paths, setPaths] = useState<Partial<Record<Media, string>>>({});
  const [errors, setErrors] = useState<FieldError[]>([]);
  const [saveErrors, setSaveErrors] = useState<FieldError[]>([]);
  const [saveFailed, setSaveFailed] = useState<BackendError | null>(null);
  /** A save in flight: the button is off and a second submit is ignored. */
  const [saving, setSaving] = useState(false);
  const inFlight = useRef(false);
  /** What is typed where the field shows something else than the profile
   * holds (art folders as a list, sizes as numbers). */
  const [raw, setRaw] = useState<Record<string, string>>({});
  const localErrors = Object.entries(raw)
    .filter(
      ([k, v]) => k.startsWith('slots.') && v.trim() && !SIZE.test(v.trim()),
    )
    .map(([field]): FieldError => ({
      field,
      code: 'invalid',
      min: null,
      max: null,
    }));

  useEffect(() => {
    let alive = true;
    const timer = setTimeout(() => {
      void getBackend()
        .then((b) => b.previewProfile(draft, example))
        .then(
          (p) => {
            if (!alive) return;
            setPaths(p.paths);
            setErrors(p.errors);
          },
          () => undefined,
        );
    }, 150);
    return () => {
      alive = false;
      clearTimeout(timer);
    };
  }, [draft, example]);

  const all = [
    ...localErrors,
    ...saveErrors.filter((e) => !errors.some((x) => x.field === e.field)),
    ...errors,
  ];
  const errorFor = (field: string) => {
    const e = all.find((x) => x.field === field);
    return e ? fieldErrorText(t, e) : null;
  };
  // The slots rule is about the profile as a whole, not a number range:
  // it gets its own words instead of the generic field text.
  const slotsError = (() => {
    const e = all.find((x) => x.field === 'slots');
    if (!e) return null;
    if (e.code === 'order') return t('profiles.errors.slotsOrder');
    if (e.code === 'required') return t('profiles.errors.slotsRequired');
    return fieldErrorText(t, e);
  })();
  const set = (patch: Partial<ProfileFile>) => {
    setSaveErrors([]);
    setDraft((d) => ({ ...d, ...patch }));
  };
  const setSlot = (m: Media, slot: SlotFile | null) => {
    setSaveErrors([]);
    setDraft((d) => {
      const slots = { ...d.slots };
      if (slot) slots[m] = slot;
      else delete slots[m];
      return { ...d, slots };
    });
  };

  const toggleSlot = (m: Media, on: boolean) => {
    // A slot switched off and on starts clean.
    setRaw((r) =>
      Object.fromEntries(
        Object.entries(r).filter(([k]) => !k.startsWith(`slots.${m}.`)),
      ),
    );
    setSlot(m, on ? EMPTY_SLOT : null);
  };

  async function save() {
    if (localErrors.length || inFlight.current) return;
    inFlight.current = true;
    setSaving(true);
    setSaveFailed(null);
    try {
      const out = await (await getBackend()).saveProfile(draft, replaces);
      if (out.errors.length) setSaveErrors(out.errors);
      else {
        onSaved(out.profiles);
        return;
      }
    } catch (e) {
      setSaveFailed(toBackendError(e));
    }
    inFlight.current = false;
    setSaving(false);
  }

  const text = (
    id: string,
    label: string,
    value: string,
    onChange: (v: string) => void,
    field: string,
  ) => {
    const error = errorFor(field);
    return (
      <div className="setting">
        <label htmlFor={id} className="setting-label">
          {label}
        </label>
        <input
          id={id}
          name={id}
          className="text-input"
          value={value}
          aria-invalid={error ? true : undefined}
          onChange={(e) => onChange(e.target.value)}
        />
        {error && (
          <p role="alert" className="field-error">
            {error}
          </p>
        )}
      </div>
    );
  };

  return (
    <form
      aria-label={t('profiles.editTitle')}
      className="profile-editor"
      onSubmit={(e) => {
        e.preventDefault();
        void save();
      }}
    >
      {text(
        'profile-id',
        t('profiles.id'),
        draft.id,
        (v) => set({ id: v }),
        'id',
      )}
      {text(
        'profile-name',
        t('profiles.name'),
        draft.name,
        (v) => set({ name: v }),
        'name',
      )}
      {text(
        'profile-roms-root',
        t('profiles.romsRoot'),
        draft.roms_root,
        (v) => set({ roms_root: v }),
        'roms_root',
      )}
      {text(
        'profile-art-dirs',
        t('profiles.artDirs'),
        raw.art_dirs ?? draft.art_dirs.join(', '),
        (v) => {
          setRaw((r) => ({ ...r, art_dirs: v }));
          set({
            art_dirs: v
              .split(',')
              .map((x) => x.trim())
              .filter(Boolean),
          });
        },
        'art_dirs',
      )}
      <label className="checkbox-row">
        <input
          type="checkbox"
          name="profile-gamelist"
          checked={draft.gamelist}
          onChange={(e) => set({ gamelist: e.target.checked })}
        />
        {t('profiles.gamelist')}
      </label>
      {slotsError && (
        <p role="alert" className="field-error">
          {slotsError}
        </p>
      )}
      {MEDIA.map((m) => {
        const slot = draft.slots[m];
        const media = t(`art.media.${m}`);
        return (
          <fieldset key={m} className="setting">
            <legend className="setting-label">
              <label className="checkbox-row">
                <input
                  type="checkbox"
                  name={`slot-${m}`}
                  checked={Boolean(slot)}
                  onChange={(e) => toggleSlot(m, e.target.checked)}
                />
                {t('profiles.slot', { media })}
              </label>
            </legend>
            {slot && (
              <>
                {text(
                  `slot-${m}-template`,
                  t('profiles.template', { media }),
                  slot.template,
                  (v) => setSlot(m, { ...slot, template: v }),
                  `slots.${m}.template`,
                )}
                <label className="field-label">
                  {t('profiles.format')}{' '}
                  <select
                    name={`slot-${m}-format`}
                    className="select"
                    value={slot.format}
                    onChange={(e) =>
                      setSlot(m, {
                        ...slot,
                        format: e.target.value as 'png' | 'jpg',
                      })
                    }
                  >
                    <option value="png">PNG</option>
                    <option value="jpg">JPEG</option>
                  </select>
                </label>
                {(['max_width', 'max_height'] as const).map((f) => {
                  const field = `slots.${m}.${f}`;
                  return (
                    <Fragment key={f}>
                      {text(
                        `slot-${m}-${f}`,
                        t(`profiles.${f}`, { media }),
                        raw[field] ?? (slot[f] === null ? '' : String(slot[f])),
                        (v) => {
                          setRaw((r) => ({ ...r, [field]: v }));
                          const n = v.trim();
                          if (n === '' || SIZE.test(n))
                            setSlot(m, {
                              ...slot,
                              [f]: n === '' ? null : Number(n),
                            });
                        },
                        field,
                      )}
                    </Fragment>
                  );
                })}
              </>
            )}
          </fieldset>
        );
      })}
      <h3 className="section-title">{t('profiles.preview')}</h3>
      <div className="preview-inputs">
        {text(
          'example-folder',
          t('profiles.exampleFolder'),
          example.system_folder,
          (v) => setExample((x) => ({ ...x, system_folder: v })),
          'example.system_folder',
        )}
        {text(
          'example-sub',
          t('profiles.exampleSub'),
          example.rel_dir,
          (v) => setExample((x) => ({ ...x, rel_dir: v })),
          'example.rel_dir',
        )}
        {text(
          'example-file',
          t('profiles.exampleFile'),
          example.file_name,
          (v) => setExample((x) => ({ ...x, file_name: v })),
          'example.file_name',
        )}
      </div>
      <dl className="preview-paths" aria-live="polite">
        {MEDIA.filter((m) => paths[m]).map((m) => (
          <div key={m}>
            <dt>{t(`art.media.${m}`)}</dt>
            <dd>
              <code>{paths[m]}</code>
            </dd>
          </div>
        ))}
      </dl>
      {saveFailed && (
        <p role="alert" className="error-banner">
          {t(errorKey(saveFailed.code), { detail: saveFailed.detail ?? '' })}
        </p>
      )}
      <div className="modal-actions">
        <button type="button" className="ghost-button" onClick={onCancel}>
          {t('profiles.cancel')}
        </button>
        <button
          type="submit"
          className="primary-button"
          disabled={saving}
          aria-busy={saving || undefined}
        >
          {t('profiles.save')}
        </button>
      </div>
    </form>
  );
}
