import { Fragment, useEffect, useRef, useState } from 'react';

import { Alert } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { Collapsible, CollapsibleContent } from '@/components/ui/collapsible';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { NativeSelect } from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';
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

import {
  errorText,
  Field,
  fieldErrorText,
  sectionTitle,
  SwitchSetting,
} from './Field';

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
    /** Paths and templates read better in the mono face. */
    mono = false,
  ) => {
    return (
      <Field id={id} label={label} error={errorFor(field)}>
        <Input
          name={id}
          mono={mono}
          value={value}
          onChange={(e) => onChange(e.target.value)}
        />
      </Field>
    );
  };

  return (
    <form
      aria-label={t('profiles.editTitle')}
      className="flex max-w-[40rem] flex-col gap-4"
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
      <SwitchSetting
        id="profile-gamelist"
        name="profile-gamelist"
        label={t('profiles.gamelist')}
        checked={draft.gamelist}
        onCheckedChange={(on) => set({ gamelist: on })}
      />
      {slotsError && (
        <p role="alert" className={errorText}>
          {slotsError}
        </p>
      )}
      {MEDIA.map((m) => {
        const slot = draft.slots[m];
        const media = t(`art.media.${m}`);
        return (
          <Collapsible key={m} open={Boolean(slot)} asChild>
            <fieldset className="m-0 grid gap-3 rounded-md border border-cr-border p-3 data-[state=closed]:border-dashed">
              <legend className="px-1">
                <span className="flex items-center gap-2.5">
                  <Switch
                    id={`slot-${m}`}
                    name={`slot-${m}`}
                    checked={Boolean(slot)}
                    onCheckedChange={(on) => toggleSlot(m, on)}
                  />
                  <Label htmlFor={`slot-${m}`}>
                    {t('profiles.slot', { media })}
                  </Label>
                </span>
              </legend>
              <CollapsibleContent className="grid gap-3">
                {slot && (
                  <>
                    {text(
                      `slot-${m}-template`,
                      t('profiles.template', { media }),
                      slot.template,
                      (v) => setSlot(m, { ...slot, template: v }),
                      `slots.${m}.template`,
                      true,
                    )}
                    <div className="grid gap-1.5">
                      <Label htmlFor={`slot-${m}-format`}>
                        {t('profiles.format')}
                      </Label>
                      <NativeSelect
                        id={`slot-${m}-format`}
                        name={`slot-${m}-format`}
                        className="max-w-40"
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
                      </NativeSelect>
                    </div>
                    <div className="grid grid-cols-2 gap-3">
                      {(['max_width', 'max_height'] as const).map((f) => {
                        const field = `slots.${m}.${f}`;
                        return (
                          <Fragment key={f}>
                            {text(
                              `slot-${m}-${f}`,
                              t(`profiles.${f}`, { media }),
                              raw[field] ??
                                (slot[f] === null ? '' : String(slot[f])),
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
                    </div>
                  </>
                )}
              </CollapsibleContent>
            </fieldset>
          </Collapsible>
        );
      })}
      <h3 className={sectionTitle}>{t('profiles.preview')}</h3>
      <div className="grid gap-3">
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
        <Alert variant="danger">
          {t(errorKey(saveFailed.code), { detail: saveFailed.detail ?? '' })}
        </Alert>
      )}
      <div className="flex flex-wrap justify-end gap-2 border-t border-cr-border pt-4">
        <Button onClick={onCancel}>{t('profiles.cancel')}</Button>
        <Button type="submit" variant="primary" loading={saving}>
          {t('profiles.save')}
        </Button>
      </div>
    </form>
  );
}
