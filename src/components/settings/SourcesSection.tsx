import { ChevronDown, ChevronUp } from 'lucide-react';
import { useCallback, useContext, useEffect, useRef, useState } from 'react';

import { Alert } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Progress } from '@/components/ui/progress';
import { Switch } from '@/components/ui/switch';
import { SimpleTooltip } from '@/components/ui/tooltip';
import { LocaleContext } from '@/i18n/core';
import { useRichT } from '@/i18n/rich';
import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import { errorKey, toBackendError, type BackendError } from '@/ipc/errors';
import {
  CREDENTIAL_KEYS,
  type CatalogStatus,
  type CredentialKey,
  type CredentialStatus,
  type FieldInput,
  type KeyCheck,
  type SourceId,
  type SourceInfo,
} from '@/ipc/types';
import { formatDecimalSize } from '@/lib/format';
import { useSettingsStore } from '@/stores/settingsStore';

import {
  AdvancedFold,
  errorText,
  Field,
  fieldErrorText,
  hintText,
  sectionTitle,
  settingId,
  SwitchSetting,
  useFieldError,
} from './Field';
import { foldCount } from './registry';
import { ReorderAnnouncer, useReorderAnnouncer } from './ReorderAnnouncer';
import { SecretField } from './SecretField';
import { SwitchCatalogStatusLine } from './SwitchCatalogStatus';

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

/** About what the catalogue downloads (`titles.<region>.<lang>.json` and
 * `ncas.json`, spec §9.2): the hint says it before it is turned on. */
const CATALOG_DOWNLOAD_BYTES = 90_000_000;

const URL_PATH = 'catalogs.switch.fallback_url';

/** How often the status is read while a download runs that this window
 * gets no answer for. */
const CATALOG_POLL_MS = 1000;

/** The catalogue holds rows: «Buscar actualización» and «Borrar catálogo». */
const hasRows = (s: CatalogStatus | null) =>
  s !== null &&
  (s.state === 'ready' ||
    s.state === 'failed-old' ||
    s.state === 'rejected' ||
    s.state === 'remove-failed');

/** The catalogue download or removal this window is waiting for. It
 * outlives the Sources tab, so a tab opened again while it runs shows it
 * running and then its answer; with none running, a `catalog-progress`
 * event is a late one (events and replies travel apart) and is dropped. */
let inflight: Promise<CatalogStatus> | null = null;

function track(p: Promise<CatalogStatus>): Promise<CatalogStatus> {
  const settled = p.finally(() => {
    if (inflight === settled) inflight = null;
  });
  inflight = settled;
  return settled;
}

/**
 * «Catálogo de Switch» (adjustments §9.2): the optional blawar/titledb name
 * catalogue, its status, download and removal, eShop icons and, folded,
 * the online fallback. Turning it off deletes it (the back end does).
 */
function SwitchCatalogGroup() {
  const t = useT();
  const tr = useRichT();
  const locale = useContext(LocaleContext);
  const c = useSettingsStore((s) => s.settings!.catalogs.switch);
  const update = useSettingsStore((s) => s.update);
  const urlError = useSettingsStore((s) => s.errors[URL_PATH]);
  const [status, setStatus] = useState<CatalogStatus | null>(null);
  const [running, setRunning] = useState(inflight !== null);
  const [error, setError] = useState<BackendError | null>(null);
  const [url, setUrl] = useState<string | null>(null);
  const alive = useRef(true);

  /** Shows the answer of `p` (a download or removal answers last: a
   * status read meanwhile is older). */
  const settle = useCallback(
    (p: Promise<CatalogStatus>) =>
      p
        .then(
          (s) => alive.current && setStatus(s),
          (e: unknown) => alive.current && setError(toBackendError(e)),
        )
        .finally(() => alive.current && setRunning(inflight !== null)),
    [],
  );

  useEffect(() => {
    alive.current = true;
    if (inflight) void settle(inflight);
    return () => {
      alive.current = false;
    };
  }, [settle]);

  useEffect(() => {
    let live = true;
    let stop: (() => void) | null = null;
    void getBackend().then(async (b) => {
      // Progress belongs to a running download: ours, or one the status
      // read reported (a plan's). Once it answered, a late event is old.
      const off = await b.onCatalogProgress(
        (s) =>
          live &&
          setStatus((now) =>
            inflight !== null || now?.state === 'downloading' ? s : now,
          ),
      );
      if (live) stop = off;
      else off();
      // While ours runs, only a read that says so is current.
      const s = await b.catalogStatus(locale).catch(() => null);
      if (live && s && (!inflight || s.state === 'downloading')) setStatus(s);
    });
    return () => {
      live = false;
      stop?.();
    };
  }, [locale, c.enabled]);

  // A download this window is not waiting for (a plan's, or one whose
  // answer this tab missed) sends no answer here: read the status until
  // it ends.
  const watching = status?.state === 'downloading' && !running;
  useEffect(() => {
    if (!watching) return;
    const timer = setInterval(() => {
      void getBackend()
        .then((b) => b.catalogStatus(locale))
        .then((s) => {
          if (!inflight) setStatus(s);
        })
        .catch(() => undefined);
    }, CATALOG_POLL_MS);
    return () => clearInterval(timer);
  }, [watching, locale]);

  function run(
    op: (b: Awaited<ReturnType<typeof getBackend>>) => Promise<CatalogStatus>,
  ) {
    setError(null);
    setRunning(true);
    return settle(track(getBackend().then(op)));
  }

  async function toggle(on: boolean) {
    setError(null);
    const errors = await update({ catalogs: { switch: { enabled: on } } });
    // On: download now (spec §7); off: the back end deletes it.
    if (on && errors.length === 0) await run((b) => b.catalogUpdate(locale));
  }

  const downloading = status?.state === 'downloading' ? status : null;
  const rows = hasRows(status);
  const typed = url ?? c.fallback_url;
  const urlMessage = !urlError
    ? null
    : urlError.code === 'invalid' && !/^https:\/\//i.test(typed.trim())
      ? t('settings.sources.switchCatalog.fallbackHttps', {
          scheme: 'https://',
        })
      : fieldErrorText(t, urlError);
  const urlId = settingId('sources.switch_catalog.fallback_url');
  return (
    <section
      role="group"
      aria-labelledby="switch-catalog-title"
      className="grid gap-4"
    >
      <h2 id="switch-catalog-title" className={sectionTitle}>
        {t('settings.sources.switchCatalog.title')}
      </h2>
      <SwitchSetting
        id={settingId('sources.switch_catalog')}
        name="switch-catalog"
        label={t('settings.sources.switchCatalog.label')}
        hint={tr('settings.sources.switchCatalog.hint', {
          size: formatDecimalSize(CATALOG_DOWNLOAD_BYTES, locale),
        })}
        checked={c.enabled}
        onCheckedChange={(on) => void toggle(on)}
      />
      <div className="grid max-w-md gap-2">
        <SwitchCatalogStatusLine
          status={c.enabled ? status : { state: 'off' }}
        />
        {downloading && (
          <Progress
            size="sm"
            value={downloading.total === null ? null : downloading.done}
            max={downloading.total ?? 100}
            aria-label={t('settings.sources.switchCatalog.label')}
          />
        )}
        <div className="flex flex-wrap gap-2">
          <Button
            size="sm"
            disabled={!c.enabled || running || downloading !== null}
            onClick={() => void run((b) => b.catalogUpdate(locale))}
          >
            {rows
              ? t('settings.sources.switchCatalog.check')
              : t('settings.sources.switchCatalog.download')}
          </Button>
          <Button
            size="sm"
            disabled={!c.enabled || running || !rows}
            onClick={() => void run((b) => b.catalogRemove())}
          >
            {t('settings.sources.switchCatalog.remove')}
          </Button>
        </div>
        {error && (
          <p role="alert" className={errorText}>
            {t(errorKey(error.code), { detail: error.detail ?? '' })}
          </p>
        )}
      </div>
      <SwitchSetting
        id={settingId('sources.switch_catalog.icons')}
        name="switch-catalog-icons"
        label={t('settings.sources.switchCatalog.icons')}
        hint={t('settings.sources.switchCatalog.iconsHint')}
        checked={c.icons}
        disabled={!c.enabled}
        onCheckedChange={(on) =>
          void update({ catalogs: { switch: { icons: on } } })
        }
      />
      <AdvancedFold
        id="sources-advanced"
        count={foldCount('sources-advanced')}
        errorPaths={['catalogs.switch']}
      >
        <SwitchSetting
          id={settingId('sources.switch_catalog.fallback')}
          name="switch-catalog-fallback"
          label={t('settings.sources.switchCatalog.fallback')}
          hint={t('settings.sources.switchCatalog.fallbackHint')}
          checked={c.fallback}
          disabled={!c.enabled}
          onCheckedChange={(on) =>
            void update({ catalogs: { switch: { fallback: on } } })
          }
        />
        <Field
          id={urlId}
          label={t('settings.sources.switchCatalog.fallbackUrl')}
          error={urlMessage}
        >
          <Input
            name={urlId}
            mono
            className="max-w-md"
            spellCheck={false}
            autoComplete="off"
            disabled={!c.enabled}
            value={typed}
            onChange={(e) => setUrl(e.target.value)}
            onBlur={async () => {
              if (url === null) return;
              const errors = await update({
                catalogs: { switch: { fallback_url: url } },
              });
              if (errors.length === 0) setUrl(null);
            }}
          />
        </Field>
      </AdvancedFold>
    </section>
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
