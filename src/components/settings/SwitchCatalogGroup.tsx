import {
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';

import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Progress } from '@/components/ui/progress';
import { LocaleContext } from '@/i18n/core';
import { useRichT } from '@/i18n/rich';
import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import { errorKey, toBackendError, type BackendError } from '@/ipc/errors';
import type { CatalogStatus } from '@/ipc/types';
import { formatDecimalSize } from '@/lib/format';
import { useSettingsStore } from '@/stores/settingsStore';

import {
  AdvancedFold,
  errorText,
  Field,
  fieldErrorText,
  settingId,
  SwitchSetting,
} from './Field';
import { foldCount, type SettingsFold } from './registry';
import { SwitchCatalogStatusLine } from './SwitchCatalogStatus';

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
 * outlives the tab, so a tab opened again while it runs shows it running
 * and then its answer; with none running, a `catalog-progress` event is a
 * late one (events and replies travel apart) and is dropped. */
let inflight: Promise<CatalogStatus> | null = null;

function track(p: Promise<CatalogStatus>): Promise<CatalogStatus> {
  const settled = p.finally(() => {
    if (inflight === settled) inflight = null;
  });
  inflight = settled;
  return settled;
}

/** What an off catalogue shows (one object: `onStatus` sees no change). */
const OFF: CatalogStatus = { state: 'off' };

/** The fold of the online fallback inside the card. */
const FOLD: SettingsFold = 'integrations-switch-advanced';

/**
 * The body of the «Catálogo de Switch» card in Integraciones (adjustments
 * §9.2, integrations D7, D23): the optional blawar/titledb name catalogue,
 * its status, download and removal, eShop icons and, folded, the online
 * fallback. Turning it off deletes it (the back end does). The card's
 * header is its title; the status it shows comes back through `onStatus`
 * (IP-1).
 */
export function SwitchCatalogGroup({
  onStatus,
}: {
  /** The status as shown (off while the setting is off). */
  onStatus?: (s: CatalogStatus | null) => void;
} = {}) {
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

  const shown = useMemo(() => (c.enabled ? status : OFF), [c.enabled, status]);
  useEffect(() => {
    onStatus?.(shown);
  }, [onStatus, shown]);

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
      ? t('integrations.switchCatalog.fallbackHttps', {
          scheme: 'https://',
        })
      : fieldErrorText(t, urlError);
  const urlId = settingId(`integrations.switch_catalog.fallback_url`);
  const sizeHint = tr('integrations.switchCatalog.hint', {
    size: formatDecimalSize(CATALOG_DOWNLOAD_BYTES, locale),
  });
  const body = (
    <>
      <SwitchSetting
        id={settingId(`integrations.switch_catalog`)}
        name="switch-catalog"
        label={t('integrations.enabled')}
        hint={
          <>
            {sizeHint} {t('integrations.catalog.offHint')}
          </>
        }
        checked={c.enabled}
        onCheckedChange={(on) => void toggle(on)}
      />
      <div className="grid max-w-md gap-2">
        <SwitchCatalogStatusLine status={shown} />
        {downloading && (
          <Progress
            size="sm"
            value={downloading.total === null ? null : downloading.done}
            max={downloading.total ?? 100}
            aria-label={t('integrations.switchCatalog.label')}
          />
        )}
        <div className="flex flex-wrap gap-2">
          <Button
            size="sm"
            disabled={!c.enabled || running || downloading !== null}
            onClick={() => void run((b) => b.catalogUpdate(locale))}
          >
            {rows
              ? t('integrations.switchCatalog.check')
              : t('integrations.switchCatalog.download')}
          </Button>
          <Button
            size="sm"
            disabled={!c.enabled || running || !rows}
            onClick={() => void run((b) => b.catalogRemove())}
          >
            {t('integrations.switchCatalog.remove')}
          </Button>
        </div>
        {error && (
          <p role="alert" className={errorText}>
            {t(errorKey(error.code), { detail: error.detail ?? '' })}
          </p>
        )}
      </div>
      <SwitchSetting
        id={settingId(`integrations.switch_catalog.icons`)}
        name="switch-catalog-icons"
        label={t('integrations.switchCatalog.icons')}
        hint={t('integrations.switchCatalog.iconsHint')}
        checked={c.icons}
        disabled={!c.enabled}
        onCheckedChange={(on) =>
          void update({ catalogs: { switch: { icons: on } } })
        }
      />
      <AdvancedFold
        id={FOLD}
        count={foldCount(FOLD)}
        errorPaths={['catalogs.switch']}
      >
        <SwitchSetting
          id={settingId(`integrations.switch_catalog.fallback`)}
          name="switch-catalog-fallback"
          label={t('integrations.switchCatalog.fallback')}
          hint={t('integrations.switchCatalog.fallbackHint')}
          checked={c.fallback}
          disabled={!c.enabled}
          onCheckedChange={(on) =>
            void update({ catalogs: { switch: { fallback: on } } })
          }
        />
        <Field
          id={urlId}
          label={t('integrations.switchCatalog.fallbackUrl')}
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
    </>
  );
  return <div className="grid gap-4">{body}</div>;
}
