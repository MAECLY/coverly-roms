import { ChevronDown, ChevronUp, Eye, EyeOff } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';

import { Alert } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import {
  FieldControl,
  FieldDescription,
  FieldError,
  FieldLabel,
  Field as UiField,
} from '@/components/ui/field';
import { Input } from '@/components/ui/input';
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
  type KeyCheck,
  type SourceId,
  type SourceInfo,
} from '@/ipc/types';
import { useSettingsStore } from '@/stores/settingsStore';

import { errorText, hintText, sectionTitle, useFieldError } from './Field';
import { ReorderAnnouncer, useReorderAnnouncer } from './ReorderAnnouncer';

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
  /** The typed key shown as text (until it is saved or cleared). */
  const [shown, setShown] = useState(false);
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
      setShown(false);
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
  const show = t('settings.sources.show', { what });
  return (
    <UiField id={id} className="gap-2">
      <FieldLabel>{what}</FieldLabel>
      <div className="flex max-w-md items-center gap-1">
        <FieldControl>
          <Input
            name={id}
            type={shown ? 'text' : 'password'}
            mono
            autoComplete="off"
            spellCheck={false}
            value={value}
            disabled={disabled}
            onChange={(e) => setValue(e.target.value)}
          />
        </FieldControl>
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
      <div className="flex flex-wrap gap-2">
        <Button
          size="sm"
          disabled={disabled || !value.trim()}
          aria-label={t('settings.sources.save', { what })}
          onClick={() => void run((b) => b.setCredential(k, value))}
        >
          {t('settings.sources.saveShort')}
        </Button>
        <Button
          size="sm"
          disabled={disabled || !set}
          aria-label={t('settings.sources.remove', { what })}
          onClick={() => void run((b) => b.clearCredential(k))}
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
      <FieldDescription>
        {set ? t('settings.sources.stored') : t('settings.sources.notStored')}
      </FieldDescription>
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
      <FieldError>
        {error && t(errorKey(error.code), { detail: error.detail ?? '' })}
      </FieldError>
    </UiField>
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
