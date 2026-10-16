import { useEffect, useState } from 'react';

import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import { errorKey, toBackendError, type BackendError } from '@/ipc/errors';
import {
  CREDENTIAL_KEYS,
  type CredentialKey,
  type CredentialStatus,
  type SourceId,
} from '@/ipc/types';
import { AVAILABLE_SOURCES } from '@/lib/settingsRules';
import { useSettingsStore } from '@/stores/settingsStore';

import { useFieldError } from './Field';

function CredentialRow({
  k,
  set,
  disabled,
  onStatus,
}: {
  k: CredentialKey;
  set: boolean;
  disabled: boolean;
  onStatus(s: CredentialStatus): void;
}) {
  const t = useT();
  const [value, setValue] = useState('');
  const [error, setError] = useState<BackendError | null>(null);
  const what = t(`settings.sources.credential.${k}`);
  const id = `credential-${k}`;
  async function run(
    op: (
      b: Awaited<ReturnType<typeof getBackend>>,
    ) => Promise<CredentialStatus>,
  ) {
    setError(null);
    try {
      onStatus(await op(await getBackend()));
    } catch (e) {
      setError(toBackendError(e));
    } finally {
      // Never keep a secret in the DOM longer than needed, even on error.
      setValue('');
    }
  }
  return (
    <div className="setting">
      <label htmlFor={id} className="setting-label">
        {what}
      </label>
      <input
        id={id}
        name={id}
        type="password"
        autoComplete="off"
        spellCheck={false}
        className="text-input"
        value={value}
        disabled={disabled}
        onChange={(e) => setValue(e.target.value)}
      />
      <div className="row-actions">
        <button
          type="button"
          className="ghost-button"
          disabled={disabled || !value.trim()}
          aria-label={t('settings.sources.save', { what })}
          onClick={() => void run((b) => b.setCredential(k, value))}
        >
          {t('settings.sources.saveShort')}
        </button>
        <button
          type="button"
          className="ghost-button"
          disabled={disabled || !set}
          aria-label={t('settings.sources.remove', { what })}
          onClick={() => void run((b) => b.clearCredential(k))}
        >
          {t('settings.sources.removeShort')}
        </button>
      </div>
      <p className="hint">
        {set ? t('settings.sources.stored') : t('settings.sources.notStored')}
      </p>
      {error && (
        <p role="alert" className="field-error">
          {t(errorKey(error.code), { detail: error.detail ?? '' })}
        </p>
      )}
    </div>
  );
}

export function SourcesSection() {
  const t = useT();
  const order = useSettingsStore((s) => s.settings!.sources.order);
  const update = useSettingsStore((s) => s.update);
  const orderError = useFieldError('sources.order');
  const [status, setStatus] = useState<CredentialStatus | null>(null);
  const [error, setError] = useState<BackendError | null>(null);
  useEffect(() => {
    let alive = true;
    void getBackend()
      .then((b) => b.credentialStatus())
      .then(
        (s) => alive && setStatus(s),
        (e) => alive && setError(toBackendError(e)),
      );
    return () => {
      alive = false;
    };
  }, []);
  const name = (id: SourceId) => t(`settings.sources.name.${id}`);
  const save = (next: typeof order) =>
    void update({ sources: { order: next } });
  const move = (i: number, d: -1 | 1) => {
    const next = [...order];
    const [x] = next.splice(i, 1);
    next.splice(i + d, 0, x!);
    save(next);
  };
  const unavailable = status !== null && !status.available;
  return (
    <div className="settings-group">
      <ol className="source-order" aria-label={t('settings.section.sources')}>
        {order.map((e, i) => {
          const usable = AVAILABLE_SOURCES.includes(e.id);
          return (
            <li key={e.id}>
              <label className="checkbox-row">
                <input
                  type="checkbox"
                  name={`source-${e.id}`}
                  checked={e.enabled}
                  disabled={!usable}
                  onChange={(ev) =>
                    save(
                      order.map((x) =>
                        x.id === e.id
                          ? { ...x, enabled: ev.target.checked }
                          : x,
                      ),
                    )
                  }
                />
                {name(e.id)}
              </label>
              {!usable && (
                <span className="hint">{t('settings.sources.later')}</span>
              )}
              <button
                type="button"
                className="icon-button"
                disabled={i === 0}
                aria-label={t('settings.sources.up', { name: name(e.id) })}
                onClick={() => move(i, -1)}
              >
                ↑
              </button>
              <button
                type="button"
                className="icon-button"
                disabled={i === order.length - 1}
                aria-label={t('settings.sources.down', { name: name(e.id) })}
                onClick={() => move(i, 1)}
              >
                ↓
              </button>
            </li>
          );
        })}
      </ol>
      {orderError && (
        <p role="alert" className="field-error">
          {orderError}
        </p>
      )}
      <h2 className="section-title">{t('settings.sources.credentials')}</h2>
      <p className="hint">{t('settings.sources.credentialsHint')}</p>
      {unavailable && (
        <p role="alert" className="error-banner">
          {t(errorKey('keychain-unavailable'))}
        </p>
      )}
      {error && (
        <p role="alert" className="error-banner">
          {t(errorKey(error.code), { detail: error.detail ?? '' })}
        </p>
      )}
      {CREDENTIAL_KEYS.map((k) => (
        <CredentialRow
          key={k}
          k={k}
          set={status?.set[k] ?? false}
          disabled={unavailable}
          onStatus={setStatus}
        />
      ))}
    </div>
  );
}
