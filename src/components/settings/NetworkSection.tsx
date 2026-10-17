import { useContext, useEffect, useState } from 'react';

import { Alert } from '@/components/ui/alert';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from '@/components/ui/alert-dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group';
import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import { errorKey, toBackendError, type BackendError } from '@/ipc/errors';
import type { ProxyChoice } from '@/ipc/types';
import { formatSize } from '@/lib/format';
import { useSettingsStore } from '@/stores/settingsStore';

import {
  Field,
  hintText,
  NumberSetting,
  settingId,
  useFieldError,
} from './Field';

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
      <fieldset className="m-0 grid gap-1.5 border-0 p-0">
        <legend
          id="network-proxy-legend"
          className="mb-1.5 p-0 text-ui font-medium text-cr-text"
        >
          {t('settings.network.proxy')}
        </legend>
        <RadioGroup
          name="proxy"
          aria-labelledby="network-proxy-legend"
          aria-describedby="network-proxy-hint"
          value={n.proxy}
          onValueChange={async (v) => {
            const p = v as ProxyChoice;
            const errors = await update({ network: { proxy: p } });
            setManualPending(p === 'manual' && errors.length > 0);
          }}
        >
          {(
            [
              'system',
              'env',
              'manual',
            ] as const satisfies readonly ProxyChoice[]
          ).map((p) => {
            const rid = settingId(`network.proxy.${p}`);
            return (
              <div key={p} className="flex items-center gap-2.5">
                <RadioGroupItem id={rid} value={p} />
                <Label htmlFor={rid} className="font-normal">
                  {t(`settings.network.proxies.${p}`)}
                </Label>
              </div>
            );
          })}
        </RadioGroup>
        <p id="network-proxy-hint" className={hintText}>
          {t('settings.network.proxyHint')}
        </p>
      </fieldset>
      <Field id={urlId} label={t('settings.network.proxyUrl')} error={urlError}>
        <Input
          name={urlId}
          mono
          className="max-w-md"
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
      <div className="grid justify-items-start gap-1.5">
        <p className={`${hintText} tabular-nums`}>
          {usage === null
            ? '—'
            : t('settings.network.inUse', { size: formatSize(usage, locale) })}
        </p>
        <AlertDialog open={confirm} onOpenChange={setConfirm}>
          <AlertDialogTrigger asChild>
            <Button>{t('settings.network.clear')}</Button>
          </AlertDialogTrigger>
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>
                {t('settings.network.clearTitle')}
              </AlertDialogTitle>
              <AlertDialogDescription>
                {t('settings.network.clearBody')}
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel>
                {t('settings.network.cancel')}
              </AlertDialogCancel>
              <AlertDialogAction
                variant="destructive"
                onClick={() => void clear()}
              >
                {t('settings.network.clearYes')}
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
        {freed !== null && (
          <p role="status" className={`${hintText} tabular-nums`}>
            {t('settings.network.freed', { size: formatSize(freed, locale) })}
          </p>
        )}
      </div>
      {error && (
        <Alert variant="danger">
          {t(errorKey(error.code), { detail: error.detail ?? '' })}
        </Alert>
      )}
      <NumberSetting
        path="network.index_refresh_hours"
        label={t('settings.network.refresh')}
        value={n.index_refresh_hours}
        toPatch={(v) => ({ network: { index_refresh_hours: v } })}
      />
    </div>
  );
}
