import { useContext, useEffect, useState } from 'react';

import { Modal } from '@/components/Modal';
import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import { errorKey, toBackendError, type BackendError } from '@/ipc/errors';
import type { ProxyChoice } from '@/ipc/types';
import { formatSize } from '@/lib/format';
import { useSettingsStore } from '@/stores/settingsStore';

import { Field, NumberSetting, settingId, useFieldError } from './Field';

export function NetworkSection() {
  const t = useT();
  const locale = useContext(LocaleContext);
  const n = useSettingsStore((s) => s.settings!.network);
  const update = useSettingsStore((s) => s.update);
  const urlError = useFieldError('network.proxy_url');
  const [url, setUrl] = useState<string | null>(null);
  // A Manual choice refused for its empty URL is applied with the URL.
  const [manualPending, setManualPending] = useState(false);
  const [usage, setUsage] = useState<number | null>(null);
  const [freed, setFreed] = useState<number | null>(null);
  const [confirm, setConfirm] = useState(false);
  const [error, setError] = useState<BackendError | null>(null);
  useEffect(() => {
    let alive = true;
    void getBackend()
      .then((b) => b.cacheUsage())
      .then(
        (u) => alive && setUsage(u),
        () => undefined,
      );
    return () => {
      alive = false;
    };
  }, []);
  async function clear() {
    setConfirm(false);
    setError(null);
    try {
      const b = await getBackend();
      setFreed(await b.clearCache());
      setUsage(await b.cacheUsage());
    } catch (e) {
      setError(toBackendError(e));
    }
  }
  const urlId = settingId('network.proxy_url');
  return (
    <div className="settings-group">
      <NumberSetting
        path="network.concurrency"
        label={t('settings.network.concurrency')}
        value={n.concurrency}
        toPatch={(v) => ({ network: { concurrency: v } })}
      />
      <NumberSetting
        path="network.timeout_secs"
        label={t('settings.network.timeout')}
        value={n.timeout_secs}
        toPatch={(v) => ({ network: { timeout_secs: v } })}
      />
      <NumberSetting
        path="network.retries"
        label={t('settings.network.retries')}
        value={n.retries}
        toPatch={(v) => ({ network: { retries: v } })}
      />
      <fieldset className="setting">
        <legend className="setting-label">{t('settings.network.proxy')}</legend>
        {(
          ['system', 'env', 'manual'] as const satisfies readonly ProxyChoice[]
        ).map((p) => (
          <label key={p} className="radio-row">
            <input
              type="radio"
              name="proxy"
              checked={n.proxy === p}
              onChange={async () => {
                const errors = await update({ network: { proxy: p } });
                setManualPending(p === 'manual' && errors.length > 0);
              }}
            />
            {t(`settings.network.proxies.${p}`)}
          </label>
        ))}
        <p className="hint">{t('settings.network.proxyHint')}</p>
      </fieldset>
      <Field id={urlId} label={t('settings.network.proxyUrl')} error={urlError}>
        <input
          id={urlId}
          name={urlId}
          className="text-input"
          value={url ?? n.proxy_url}
          placeholder="http://proxy:3128"
          onChange={(e) => setUrl(e.target.value)}
          onBlur={async () => {
            if (url === null) return;
            const patch = manualPending
              ? { proxy: 'manual' as const, proxy_url: url }
              : { proxy_url: url };
            if ((await update({ network: patch })).length === 0) {
              setUrl(null);
              setManualPending(false);
            }
          }}
        />
      </Field>
      <NumberSetting
        path="network.cache_max_mb"
        label={t('settings.network.cacheMax')}
        value={n.cache_max_mb}
        toPatch={(v) => ({ network: { cache_max_mb: v } })}
      />
      <p className="hint">
        {usage === null
          ? '—'
          : t('settings.network.inUse', { size: formatSize(usage, locale) })}
      </p>
      <button
        type="button"
        className="ghost-button"
        onClick={() => setConfirm(true)}
      >
        {t('settings.network.clear')}
      </button>
      {freed !== null && (
        <p role="status" className="hint">
          {t('settings.network.freed', { size: formatSize(freed, locale) })}
        </p>
      )}
      {error && (
        <p role="alert" className="error-banner">
          {t(errorKey(error.code), { detail: error.detail ?? '' })}
        </p>
      )}
      <NumberSetting
        path="network.index_refresh_hours"
        label={t('settings.network.refresh')}
        value={n.index_refresh_hours}
        toPatch={(v) => ({ network: { index_refresh_hours: v } })}
      />
      {confirm && (
        <Modal labelledBy="clear-cache-title" onClose={() => setConfirm(false)}>
          <h2 id="clear-cache-title">{t('settings.network.clearTitle')}</h2>
          <p>{t('settings.network.clearBody')}</p>
          <div className="modal-actions">
            <button
              type="button"
              className="ghost-button"
              onClick={() => setConfirm(false)}
            >
              {t('settings.network.cancel')}
            </button>
            <button
              type="button"
              className="primary-button"
              onClick={() => void clear()}
            >
              {t('settings.network.clearYes')}
            </button>
          </div>
        </Modal>
      )}
    </div>
  );
}
