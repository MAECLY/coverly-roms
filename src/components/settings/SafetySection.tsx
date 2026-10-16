import { useState } from 'react';

import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import { errorKey, toBackendError, type BackendError } from '@/ipc/errors';
import { useSettingsStore } from '@/stores/settingsStore';

import { Field, NumberSetting, settingId, useFieldError } from './Field';

export function SafetySection() {
  const t = useT();
  const s = useSettingsStore((st) => st.settings!.safety);
  const update = useSettingsStore((st) => st.update);
  const dirError = useFieldError('safety.backup_dir');
  const [dir, setDir] = useState<string | null>(null);
  const [error, setError] = useState<BackendError | null>(null);
  const commitDir = async (value: string) => {
    if (
      (await update({ safety: { backup_dir: value.trim() || null } }))
        .length === 0
    )
      setDir(null);
  };
  const id = settingId('safety.backup_dir');
  return (
    <div className="settings-group">
      <label className="checkbox-row">
        <input
          type="checkbox"
          name="backup"
          checked
          disabled
          aria-describedby="backup-locked"
        />
        {t('settings.safety.backup')}
      </label>
      <p id="backup-locked" className="hint">
        {t('settings.safety.backupHint')}
      </p>
      <Field
        id={id}
        label={t('settings.safety.backupDir')}
        hint={t('settings.safety.backupDirHint')}
        error={dirError}
      >
        <input
          id={id}
          name={id}
          className="text-input"
          value={dir ?? s.backup_dir ?? ''}
          onChange={(e) => setDir(e.target.value)}
          onBlur={() => dir !== null && void commitDir(dir)}
        />
      </Field>
      <button
        type="button"
        className="ghost-button"
        onClick={async () => {
          setError(null);
          let picked: string | null;
          try {
            picked = await (await getBackend()).pickFolder();
          } catch (e) {
            setError(toBackendError(e));
            return;
          }
          if (picked) await commitDir(picked);
        }}
      >
        {t('settings.safety.choose')}
      </button>
      {error && (
        <p role="alert" className="error-banner">
          {t(errorKey(error.code), { detail: error.detail ?? '' })}
        </p>
      )}
      <label className="checkbox-row">
        <input
          type="checkbox"
          name="dry-run"
          checked={s.dry_run_default}
          onChange={(e) =>
            void update({ safety: { dry_run_default: e.target.checked } })
          }
        />
        {t('settings.safety.dryRun')}
      </label>
      <label className="checkbox-row">
        <input
          type="checkbox"
          name="apple-double"
          checked={s.clean_apple_double}
          onChange={(e) =>
            void update({ safety: { clean_apple_double: e.target.checked } })
          }
        />
        {t('settings.safety.appleDouble')}
      </label>
      <NumberSetting
        path="safety.undo_depth"
        label={t('settings.safety.undoDepth')}
        value={s.undo_depth}
        toPatch={(v) => ({ safety: { undo_depth: v } })}
      />
    </div>
  );
}
