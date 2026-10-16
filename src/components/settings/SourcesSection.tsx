import { useEffect, useRef, useState } from 'react';

import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import { errorKey, toBackendError, type BackendError } from '@/ipc/errors';
import {
  CREDENTIAL_KEYS,
  type CredentialKey,
  type CredentialStatus,
  type KeyCheck,
  type SourceId,
  type SourceInfo,
} from '@/ipc/types';
import { useSettingsStore } from '@/stores/settingsStore';

import { useFieldError } from './Field';

function CredentialRow({
  k,
  set,
  disabled,
  testable,
  onStatus,
}: {
  k: CredentialKey;
  set: boolean;
  disabled: boolean;
  /** The source whose key "Test" checks; no Test button without it. */
  testable?: SourceId | undefined;
  onStatus(s: CredentialStatus): void;
}) {
  const t = useT();
  const [value, setValue] = useState('');
  const [error, setError] = useState<BackendError | null>(null);
  const [testing, setTesting] = useState(false);
  const [check, setCheck] = useState<KeyCheck | null>(null);
  /**
   * Bumped by every save, remove and test: a test answer whose number is
   * no longer current is about a key that has changed since, so it is
   * dropped.
   */
  const generation = useRef(0);
  const what = t(`settings.sources.credential.${k}`);
  const id = `credential-${k}`;
  async function run(
    op: (
      b: Awaited<ReturnType<typeof getBackend>>,
    ) => Promise<CredentialStatus>,
  ) {
    setError(null);
    // A result about the previous key would be stale, and so would a test
    // still running for it.
    generation.current += 1;
    setCheck(null);
    setTesting(false);
    try {
      onStatus(await op(await getBackend()));
    } catch (e) {
      setError(toBackendError(e));
    } finally {
      // Never keep a secret in the DOM longer than needed, even on error.
      setValue('');
    }
  }
  async function test(source: SourceId) {
    const mine = ++generation.current;
    const current = () => generation.current === mine;
    setTesting(true);
    setCheck(null);
    setError(null);
    try {
      // Only the verdict comes back; the key never reaches the webview.
      const verdict = await (await getBackend()).testSourceKey(source);
      if (current()) setCheck(verdict);
    } catch (e) {
      if (current()) setError(toBackendError(e));
    } finally {
      if (current()) setTesting(false);
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
        {testable && (
          <button
            type="button"
            className="ghost-button"
            disabled={disabled || !set || testing}
            aria-label={t('settings.sources.testKeyLabel', { what })}
            onClick={() => void test(testable)}
          >
            {testing
              ? t('settings.sources.testing')
              : t('settings.sources.testKey')}
          </button>
        )}
      </div>
      <p className="hint">
        {set ? t('settings.sources.stored') : t('settings.sources.notStored')}
      </p>
      {check && (
        <p role="status" className="hint">
          {t(`settings.sources.check.${check}`)}
        </p>
      )}
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
  const [info, setInfo] = useState<SourceInfo[] | null>(null);
  const [error, setError] = useState<BackendError | null>(null);
  useEffect(() => {
    let alive = true;
    const fail = (e: unknown) => alive && setError(toBackendError(e));
    void getBackend().then((b) => {
      void b.credentialStatus().then((s) => alive && setStatus(s), fail);
      void b.sourcesInfo().then((i) => alive && setInfo(i), fail);
    }, fail);
    return () => {
      alive = false;
    };
  }, []);
  const name = (id: SourceId) => t(`settings.sources.name.${id}`);
  const infoOf = (id: SourceId) => info?.find((x) => x.id === id);
  // Readiness follows the live key status, so saving or removing a key
  // updates it at once (no reload).
  const ready = (id: SourceId) => {
    const i = infoOf(id);
    return !!i && i.built && i.needs.every((k) => status?.set[k] === true);
  };
  const built = (id: SourceId) => infoOf(id)?.built === true;
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
          // An enabled source can always be turned off.
          const usable = ready(e.id) || e.enabled;
          const hint =
            info === null
              ? null
              : !built(e.id)
                ? t('settings.sources.notBuilt')
                : !ready(e.id)
                  ? t('settings.sources.needsKey')
                  : null;
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
              {hint && <span className="hint">{hint}</span>}
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
          testable={
            k === 'steamgriddb-api-key'
              ? 'steamgriddb'
              : k === 'screenscraper-password' && built('screenscraper')
                ? 'screenscraper'
                : undefined
          }
          onStatus={setStatus}
        />
      ))}
    </div>
  );
}
