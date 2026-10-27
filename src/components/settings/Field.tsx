import { ChevronRight } from 'lucide-react';
import { useEffect, useRef, useState, type ReactNode } from 'react';

import { Button } from '@/components/ui/button';
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible';
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
import { useNavStore } from '@/stores/navStore';
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
  more,
  action,
  error,
  className,
  children,
}: {
  id: string;
  label: string;
  hint?: string;
  /** Long help behind «Más información», after the hint (D7). */
  more?: ReactNode;
  /** A button on the control's row (e.g. «Elegir…»), wrapping below it
   * on a narrow window. */
  action?: ReactNode;
  error?: string | null;
  className?: string;
  children: ReactNode;
}) {
  return (
    <UiField id={id} className={className} data-setting-row="">
      <FieldLabel>{label}</FieldLabel>
      {action ? (
        <div className="flex flex-wrap items-center gap-2">
          <FieldControl>{children}</FieldControl>
          {action}
        </div>
      ) : (
        <FieldControl>{children}</FieldControl>
      )}
      {hint && <FieldDescription>{hint}</FieldDescription>}
      <FieldErrorText>{error}</FieldErrorText>
      {more && <MoreInfo id={id}>{more}</MoreInfo>}
    </UiField>
  );
}

/** A setting that applies at once: a Switch with its label (and hint). */
export function SwitchSetting({
  id,
  name,
  label,
  hint,
  more,
  checked,
  disabled,
  onCheckedChange,
}: {
  id: string;
  name: string;
  label: string;
  hint?: ReactNode;
  /** Long help behind «Más información», after the hint (D7). */
  more?: ReactNode;
  checked: boolean;
  disabled?: boolean;
  onCheckedChange?: (on: boolean) => void;
}) {
  return (
    <UiField
      id={id}
      orientation="horizontal"
      disabled={disabled ?? false}
      data-setting-row=""
    >
      <FieldControl>
        <Switch
          name={name}
          checked={checked}
          {...(onCheckedChange ? { onCheckedChange } : {})}
        />
      </FieldControl>
      <FieldLabel className="font-normal">{label}</FieldLabel>
      {hint && <FieldDescription>{hint}</FieldDescription>}
      {more && (
        <div className="col-start-2">
          <MoreInfo id={id}>{more}</MoreInfo>
        </div>
      )}
    </UiField>
  );
}

const MARK = '\u0000';

/** A translated line with one value in the data face (brand §5): `text`
 * holds a `\u0000` where the value goes. */
export function MonoIn({
  text,
  value,
  className,
}: {
  text: string;
  value: ReactNode;
  /** Extra classes on the value span (e.g. wrapping long paths). */
  className?: string;
}) {
  const [before = '', after = ''] = text.split(MARK);
  return (
    <>
      {before}
      <span className={className ? `font-mono ${className}` : 'font-mono'}>
        {value}
      </span>
      {after}
    </>
  );
}

/** The long help behind a text toggle, after the short hint (D7): the
 * control's aria-describedby keeps reading only the short one. */
export function MoreInfo({
  id,
  children,
}: {
  id: string;
  children: ReactNode;
}) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const panel = `${id}-more`;
  return (
    <div className="grid gap-1">
      <Button
        variant="link"
        size="sm"
        className="h-auto justify-self-start p-0"
        aria-expanded={open}
        aria-controls={panel}
        onClick={() => setOpen((o) => !o)}
      >
        {t(open ? 'settings.lessInfo' : 'settings.moreInfo')}
      </Button>
      <p id={panel} hidden={!open} className={hintText}>
        {children}
      </p>
    </div>
  );
}

/** «Opciones avanzadas»: closed by default, open for the session once
 * opened (navStore.openFolds), content kept mounted so drafts and ids
 * survive (D5). Opens itself when a field inside gets an error, and says
 * so on the trigger while closed. */
export function AdvancedFold({
  id,
  count,
  errorPaths,
  children,
}: {
  /** 'library-advanced' | 'organize-advanced' | 'duplicates-advanced' */
  id: string;
  /** Registry entries inside (shown in mono). */
  count: number;
  /** Settings paths whose errors (path or path.*) live inside. */
  errorPaths: readonly string[];
  children: ReactNode;
}) {
  const t = useT();
  const open = useNavStore((s) => s.openFolds.includes(id));
  const setFold = useNavStore((s) => s.setFold);
  const hasError = useSettingsStore((s) =>
    Object.keys(s.errors).some((k) =>
      errorPaths.some((p) => k === p || k.startsWith(`${p}.`)),
    ),
  );
  const had = useRef(false);
  useEffect(() => {
    if (hasError && !had.current) setFold(id, true);
    had.current = hasError;
  }, [hasError, id, setFold]);
  const label = t('settings.advancedOptions');
  const name = `${label} (${count})`;
  const flagged = hasError && !open;
  return (
    <Collapsible
      open={open}
      onOpenChange={(o) => setFold(id, o)}
      data-advanced-fold={id}
      className="grid gap-4"
    >
      <CollapsibleTrigger asChild>
        <Button
          variant="ghost"
          className="group justify-self-start px-1"
          aria-label={
            flagged ? t('settings.advancedHasErrors', { label: name }) : name
          }
        >
          <ChevronRight
            aria-hidden
            className="transition-transform duration-fast ease-standard group-data-[state=open]:rotate-90 motion-reduce:transition-none"
          />
          {label} <span className="font-mono">({count})</span>
          {flagged && (
            <span
              data-fold-error=""
              aria-hidden
              className="size-1.5 rounded-full bg-cr-danger"
            />
          )}
        </Button>
      </CollapsibleTrigger>
      {/* forceMount keeps Radix from hiding it: hide it ourselves, and keep
          `grid` from overriding the hidden attribute. */}
      <CollapsibleContent
        forceMount
        hidden={!open}
        className="settings-fold__content grid gap-4 data-[state=closed]:hidden"
      >
        {children}
      </CollapsibleContent>
    </Collapsible>
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
