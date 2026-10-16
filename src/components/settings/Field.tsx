import { useState, type ReactNode } from 'react';

import { useT } from '@/i18n/useT';
import type { FieldError, SettingsPatch } from '@/ipc/types';
import { useSettingsStore } from '@/stores/settingsStore';

type T = ReturnType<typeof useT>;

// eslint-disable-next-line react-refresh/only-export-components -- shared by every settings section (plan interface)
export const settingId = (path: string) =>
  `setting-${path.replace(/[^a-zA-Z0-9]+/g, '-')}`;

// eslint-disable-next-line react-refresh/only-export-components -- shared by every settings section (plan interface)
export function fieldErrorText(t: T, e: FieldError): string {
  return t(`errors.field.${e.code}`, { min: e.min ?? '', max: e.max ?? '' });
}

// eslint-disable-next-line react-refresh/only-export-components -- shared by every settings section (plan interface)
export function useFieldError(path: string): string | null {
  const t = useT();
  const e = useSettingsStore((s) => s.errors[path]);
  return e ? fieldErrorText(t, e) : null;
}

export function Field({
  id,
  label,
  hint,
  error,
  children,
}: {
  id: string;
  label: string;
  hint?: string;
  error?: string | null;
  children: ReactNode;
}) {
  return (
    <div className="setting">
      <label htmlFor={id} className="setting-label">
        {label}
      </label>
      {children}
      {hint && (
        <p id={`${id}-hint`} className="hint">
          {hint}
        </p>
      )}
      {error && (
        <p id={`${id}-error`} role="alert" className="field-error">
          {error}
        </p>
      )}
    </div>
  );
}

/** A number committed on blur or Enter; the typed text stays when the
 * backend refuses it, with the error under the field. */
export function NumberSetting({
  path,
  label,
  hint,
  value,
  scale = 1,
  toPatch,
}: {
  path: string;
  label: string;
  hint?: string;
  value: number;
  /** Shown value = stored value × scale (e.g. 100 for a percentage). */
  scale?: number;
  toPatch(stored: number): SettingsPatch;
}) {
  const t = useT();
  const id = settingId(path);
  const update = useSettingsStore((s) => s.update);
  const remote = useFieldError(path);
  const [draft, setDraft] = useState<string | null>(null);
  const [local, setLocal] = useState<string | null>(null);
  const error = local ?? remote;
  async function commit() {
    if (draft === null) return;
    const n = Number(draft.trim().replace(',', '.'));
    if (draft.trim() === '' || !Number.isFinite(n)) {
      setLocal(t('errors.field.invalid'));
      return;
    }
    setLocal(null);
    const errors = await update(toPatch(Math.round((n / scale) * 1000) / 1000));
    if (errors.length === 0) setDraft(null);
  }
  return (
    <Field id={id} label={label} {...(hint ? { hint } : {})} error={error}>
      <input
        id={id}
        name={id}
        type="text"
        inputMode="decimal"
        className="text-input setting-number"
        value={draft ?? String(Math.round(value * scale * 100) / 100)}
        aria-invalid={error ? true : undefined}
        aria-describedby={
          [hint ? `${id}-hint` : '', error ? `${id}-error` : '']
            .filter(Boolean)
            .join(' ') || undefined
        }
        onChange={(e) => setDraft(e.target.value)}
        onBlur={() => void commit()}
        onKeyDown={(e) => {
          if (e.key === 'Enter') void commit();
        }}
      />
    </Field>
  );
}
