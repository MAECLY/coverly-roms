import { useState } from 'react';

import { Modal } from '@/components/Modal';
import { useT } from '@/i18n/useT';
import { getBackend, type Backend } from '@/ipc/backend';
import { errorKey, toBackendError, type BackendError } from '@/ipc/errors';
import type { LogLevel } from '@/ipc/types';
import { useSettingsStore } from '@/stores/settingsStore';

import { Field, settingId, useFieldError } from './Field';

export function AdvancedSection() {
  const t = useT();
  const a = useSettingsStore((s) => s.settings!.advanced);
  const update = useSettingsStore((s) => s.update);
  const reset = useSettingsStore((s) => s.reset);
  const dirError = useFieldError('advanced.custom_dir');
  const [dir, setDir] = useState<string | null>(null);
  const [confirm, setConfirm] = useState(false);
  const [error, setError] = useState<BackendError | null>(null);
  async function openFolder(op: (b: Backend) => Promise<void>) {
    setError(null);
    try {
      await op(await getBackend());
    } catch (e) {
      setError(toBackendError(e));
    }
  }
  const id = settingId('advanced.custom_dir');
  return (
    <div className="settings-group">
      <Field
        id={settingId('advanced.log_level')}
        label={t('settings.advanced.logLevel')}
      >
        <select
          id={settingId('advanced.log_level')}
          name="log-level"
          className="select"
          value={a.log_level}
          onChange={(e) =>
            void update({ advanced: { log_level: e.target.value as LogLevel } })
          }
        >
          {(['error', 'warn', 'info', 'debug'] as const).map((l) => (
            <option key={l} value={l}>
              {t(`settings.advanced.levels.${l}`)}
            </option>
          ))}
        </select>
      </Field>
      <div className="row-actions">
        <button
          type="button"
          className="ghost-button"
          onClick={() => void openFolder((b) => b.openLogsFolder())}
        >
          {t('settings.advanced.openLogs')}
        </button>
        <button
          type="button"
          className="ghost-button"
          onClick={() => void openFolder((b) => b.openConfigFolder())}
        >
          {t('settings.advanced.openConfig')}
        </button>
      </div>
      {error && (
        <p role="alert" className="error-banner">
          {t(errorKey(error.code), { detail: error.detail ?? '' })}
        </p>
      )}
      <Field
        id={id}
        label={t('settings.advanced.customDir')}
        hint={t('settings.advanced.customDirHint')}
        error={dirError}
      >
        <input
          id={id}
          name={id}
          className="text-input"
          value={dir ?? a.custom_dir ?? ''}
          onChange={(e) => setDir(e.target.value)}
          onBlur={async () => {
            if (dir === null) return;
            if (
              (await update({ advanced: { custom_dir: dir.trim() || null } }))
                .length === 0
            )
              setDir(null);
          }}
        />
      </Field>
      <button
        type="button"
        className="ghost-button danger"
        onClick={() => setConfirm(true)}
      >
        {t('settings.advanced.reset')}
      </button>
      {confirm && (
        <Modal labelledBy="reset-title" onClose={() => setConfirm(false)}>
          <h2 id="reset-title">{t('settings.advanced.resetTitle')}</h2>
          <p>{t('settings.advanced.resetBody')}</p>
          <div className="modal-actions">
            <button
              type="button"
              className="ghost-button"
              onClick={() => setConfirm(false)}
            >
              {t('settings.advanced.cancel')}
            </button>
            <button
              type="button"
              className="primary-button"
              onClick={() => {
                setConfirm(false);
                void reset();
              }}
            >
              {t('settings.advanced.resetYes')}
            </button>
          </div>
        </Modal>
      )}
    </div>
  );
}
