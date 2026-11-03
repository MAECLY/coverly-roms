import { ChevronDown, ChevronUp } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';

import { Alert } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';
import { SimpleTooltip } from '@/components/ui/tooltip';
import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import { errorKey, toBackendError, type BackendError } from '@/ipc/errors';
import {
  CREDENTIAL_KEYS,
  type CredentialKey,
  type CredentialStatus,
  type FieldInput,
  type KeyCheck,
  type SourceId,
  type SourceInfo,
} from '@/ipc/types';
import { useSettingsStore } from '@/stores/settingsStore';

import { errorText, hintText, sectionTitle, useFieldError } from './Field';
import { ReorderAnnouncer, useReorderAnnouncer } from './ReorderAnnouncer';
import { SecretField } from './SecretField';
import { SwitchCatalogGroup } from './SwitchCatalogGroup';

/** The input a credential key takes (its gender picks «Guardado/Guardada»). */
const CREDENTIAL_INPUT: Record<CredentialKey, FieldInput> = {
  'steamgriddb-api-key': 'api-key',
  'screenscraper-user': 'user',
  'screenscraper-password': 'password',
};

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
  /** A result about the previous key would be stale, and so would a test
   * still running for it. */
  function forgetCheck() {
    setError(null);
    generation.current += 1;
    setCheck(null);
    setTesting(false);
  }
  async function save(value: string) {
    forgetCheck();
    // A failure is shown by the field itself.
    onStatus(await (await getBackend()).setCredential(k, value));
  }
  async function remove() {
    forgetCheck();
    try {
      onStatus(await (await getBackend()).clearCredential(k));
    } catch (e) {
      setError(toBackendError(e));
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
    <div className="grid gap-2">
      <SecretField
        k={k}
        input={CREDENTIAL_INPUT[k]}
        set={set}
        disabled={disabled}
        label={what}
        onSave={save}
      />
      <div className="flex flex-wrap gap-2">
        <Button
          size="sm"
          disabled={disabled || !set}
          aria-label={t('settings.sources.remove', { what })}
          onClick={() => void remove()}
        >
          {t('settings.sources.removeShort')}
        </Button>
        {testable && (
          <Button
            size="sm"
            disabled={disabled || !set || testing}
            aria-label={t('settings.sources.testKeyLabel', { what })}
            onClick={() => void test(testable)}
          >
            {testing
              ? t('settings.sources.testing')
              : t('settings.sources.testKey')}
          </Button>
        )}
      </div>
      {check && (
        <p
          role="status"
          className={
            check === 'ok'
              ? 'm-0 text-caption font-medium text-cr-success'
              : errorText
          }
        >
          {t(`settings.sources.check.${check}`)}
        </p>
      )}
      {error && (
        <p role="alert" className={errorText}>
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
    announcer.moved(name(x!.id), i + d + 1, next.length);
  };
  const unavailable = status !== null && !status.available;
  const announcer = useReorderAnnouncer();
  return (
    <div className="settings-group">
      <ol
        className="m-0 grid list-decimal gap-1 pl-6 marker:text-caption marker:text-cr-text-secondary"
        aria-label={t('settings.section.sources')}
        aria-describedby={orderError ? 'sources-order-error' : undefined}
      >
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
                  : e.id === 'embedded'
                    ? // A source with no key or account says what it uses.
                      t('settings.sources.hint.embedded')
                    : null;
          const sid = `source-${e.id}`;
          const up = t('settings.sources.up', { name: name(e.id) });
          const down = t('settings.sources.down', { name: name(e.id) });
          return (
            <li key={e.id} className="pl-1">
              <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1">
                <Switch
                  id={sid}
                  name={sid}
                  checked={e.enabled}
                  disabled={!usable}
                  aria-describedby={hint ? `${sid}-hint` : undefined}
                  onCheckedChange={(on) =>
                    save(
                      order.map((x) =>
                        x.id === e.id ? { ...x, enabled: on } : x,
                      ),
                    )
                  }
                />
                <Label htmlFor={sid} className="font-normal">
                  {name(e.id)}
                </Label>
                {hint && (
                  <span id={`${sid}-hint`} className={hintText}>
                    {hint}
                  </span>
                )}
                <span className="ml-auto flex gap-1">
                  <SimpleTooltip content={up}>
                    <Button
                      variant="ghost"
                      size="icon-sm"
                      disabled={i === 0}
                      aria-label={up}
                      onClick={() => move(i, -1)}
                    >
                      <ChevronUp aria-hidden />
                    </Button>
                  </SimpleTooltip>
                  <SimpleTooltip content={down}>
                    <Button
                      variant="ghost"
                      size="icon-sm"
                      disabled={i === order.length - 1}
                      aria-label={down}
                      onClick={() => move(i, 1)}
                    >
                      <ChevronDown aria-hidden />
                    </Button>
                  </SimpleTooltip>
                </span>
              </div>
            </li>
          );
        })}
      </ol>
      <ReorderAnnouncer message={announcer.message} />
      {orderError && (
        <p id="sources-order-error" role="alert" className={errorText}>
          {orderError}
        </p>
      )}
      <SwitchCatalogGroup />
      <h2 className={sectionTitle}>{t('settings.sources.credentials')}</h2>
      <p className={hintText}>{t('settings.sources.credentialsHint')}</p>
      {unavailable && (
        <Alert variant="danger">{t(errorKey('keychain-unavailable'))}</Alert>
      )}
      {error && (
        <Alert variant="danger">
          {t(errorKey(error.code), { detail: error.detail ?? '' })}
        </Alert>
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
