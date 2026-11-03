import { Check, Eye, EyeOff } from 'lucide-react';
import { useState } from 'react';

import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { SimpleTooltip } from '@/components/ui/tooltip';
import { useT } from '@/i18n/useT';
import { errorKey, toBackendError, type BackendError } from '@/ipc/errors';
import type { CredentialKey, FieldInput } from '@/ipc/types';

import { errorText, hintText } from './Field';

/**
 * One credential input (spec D8, §6.4): masked, never refilled with the
 * stored value (the keychain value never reaches the webview; only «set»
 * comes back), with «Mostrar» for what is being typed and its own
 * «Guardar». The field empties itself after every save, failed or not, so
 * a secret stays in the DOM no longer than needed. Whatever has to happen
 * around a save (dropping a stale test answer) is the caller's `onSave`.
 */
export function SecretField({
  k,
  input,
  set,
  disabled,
  onSave,
  describedBy,
  label,
}: {
  k: CredentialKey;
  input: FieldInput;
  /** A value is stored in the keychain for `k`. */
  set: boolean;
  disabled: boolean;
  /** Called with the typed value; resolves when stored. The field empties
   * itself after. A rejection's error is shown under the field. */
  onSave(value: string): Promise<void>;
  /** Extra description of the input (the card's state line). */
  describedBy?: string | undefined;
  /** The label, when the input's generic name is not enough. */
  label?: string | undefined;
}) {
  const t = useT();
  const [value, setValue] = useState('');
  const [shown, setShown] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<BackendError | null>(null);
  const id = `secret-${k}`;
  const savedId = `${id}-saved`;
  const errorId = `${id}-error`;
  const what = label ?? t(`integrations.field.${input}.label`);
  const show = t('integrations.show', { what });

  async function save() {
    setSaving(true);
    setError(null);
    try {
      await onSave(value);
    } catch (e) {
      setError(toBackendError(e));
    } finally {
      // Never keep a secret in the DOM longer than needed, even on error.
      setValue('');
      setShown(false);
      setSaving(false);
    }
  }

  const message =
    error &&
    (error.code === 'bad-request'
      ? t('integrations.badValue')
      : t(errorKey(error.code), { detail: error.detail ?? '' }));
  const description =
    [describedBy, set && savedId, error && errorId].filter(Boolean).join(' ') ||
    undefined;

  return (
    <div className="grid gap-1.5">
      <Label htmlFor={id} className={disabled ? 'text-cr-text-disabled' : ''}>
        {what}
      </Label>
      <div className="flex flex-wrap items-center gap-x-2 gap-y-1.5">
        <div className="flex min-w-0 max-w-md flex-[1_1_14rem] items-center gap-1">
          <Input
            id={id}
            name={`integration-${k}`}
            type={shown ? 'text' : 'password'}
            mono
            autoComplete="off"
            spellCheck={false}
            value={value}
            disabled={disabled}
            // What is being saved is what was typed: no edits mid-save.
            readOnly={saving}
            aria-describedby={description}
            aria-invalid={error ? true : undefined}
            onChange={(e) => {
              setValue(e.target.value);
              // An emptied field masks the next value: the toggle is off
              // while there is nothing to show.
              if (!e.target.value) setShown(false);
              if (error) setError(null);
            }}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && value.trim() && !saving) {
                e.preventDefault();
                void save();
              }
            }}
          />
          <SimpleTooltip content={show}>
            <Button
              variant="ghost"
              size="icon"
              aria-label={show}
              aria-pressed={shown}
              aria-controls={id}
              disabled={disabled || !value}
              onClick={() => setShown((v) => !v)}
            >
              {shown ? <EyeOff aria-hidden /> : <Eye aria-hidden />}
            </Button>
          </SimpleTooltip>
        </div>
        <Button
          size="sm"
          disabled={disabled || saving || !value.trim()}
          aria-label={t('integrations.saveWhat', { what })}
          onClick={() => void save()}
        >
          {saving ? t('integrations.saving') : t('integrations.save')}
        </Button>
      </div>
      {set && (
        <p
          id={savedId}
          className={`${hintText} inline-flex items-center gap-1 [&_svg]:size-3.5 [&_svg]:text-cr-success`}
        >
          <Check aria-hidden />
          {t(`integrations.field.${input}.saved`)}
        </p>
      )}
      {message && (
        <p id={errorId} role="alert" className={errorText}>
          {message}
        </p>
      )}
    </div>
  );
}
