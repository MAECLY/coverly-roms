import { useState, type ReactNode } from 'react';

import {
  FieldControl,
  FieldDescription,
  FieldError as FieldErrorText,
  FieldLabel,
  Field as UiField,
} from '@/components/ui/field';
import { Input } from '@/components/ui/input';
import { Switch } from '@/components/ui/switch';
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

/** Group heading inside a settings panel (a shelf label). */
export const sectionTitle =
  'm-0 text-dense font-semibold text-cr-text [&:not(:first-child)]:mt-2';

/** Secondary text under a control or a group. */
export const hintText = 'm-0 text-caption text-cr-text-secondary';

/** An inline error that is not tied to one control (a list, a group). */
export const errorText = 'm-0 text-caption font-medium text-cr-danger';

/**
 * Label + control + hint + error. The one child is the control: it gets the
 * `id`, `aria-describedby` (hint, then error) and `aria-invalid`.
 */
export function Field({
  id,
  label,
  hint,
  error,
  className,
  children,
}: {
  id: string;
  label: string;
  hint?: string;
  error?: string | null;
  className?: string;
  children: ReactNode;
}) {
  return (
    <UiField id={id} className={className}>
      <FieldLabel>{label}</FieldLabel>
      <FieldControl>{children}</FieldControl>
      {hint && <FieldDescription>{hint}</FieldDescription>}
      <FieldErrorText>{error}</FieldErrorText>
    </UiField>
  );
}

/** A setting that applies at once: a Switch with its label (and hint). */
export function SwitchSetting({
  id,
  name,
  label,
  hint,
  checked,
  disabled,
  onCheckedChange,
}: {
  id: string;
  name: string;
  label: string;
  hint?: ReactNode;
  checked: boolean;
  disabled?: boolean;
  onCheckedChange?: (on: boolean) => void;
}) {
  return (
    <UiField id={id} orientation="horizontal" disabled={disabled ?? false}>
      <FieldControl>
        <Switch
          name={name}
          checked={checked}
          {...(onCheckedChange ? { onCheckedChange } : {})}
        />
      </FieldControl>
      <FieldLabel className="font-normal">{label}</FieldLabel>
      {hint && <FieldDescription>{hint}</FieldDescription>}
    </UiField>
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
      <Input
        name={id}
        type="text"
        inputMode="decimal"
        className="max-w-40 tabular-nums"
        value={draft ?? String(Math.round(value * scale * 100) / 100)}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={() => void commit()}
        onKeyDown={(e) => {
          if (e.key === 'Enter') void commit();
        }}
      />
    </Field>
  );
}
