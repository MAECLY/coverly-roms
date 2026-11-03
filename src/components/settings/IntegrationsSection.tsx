import { ExternalLink, KeyRound } from 'lucide-react';
import { useCallback, useContext, useEffect, useRef, useState } from 'react';

import { Alert } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import { errorKey, toBackendError, type BackendError } from '@/ipc/errors';
import type { CatalogStatus, IntegrationView } from '@/ipc/types';
import {
  catalogCardState,
  integrationDomId,
  PLANNED,
  updatesCardState,
  WORKING,
  type Integration,
  type IntegrationGroup,
} from '@/lib/integrations';
import { cn } from '@/lib/utils';
import { useNavStore } from '@/stores/navStore';
import { useSettingsStore } from '@/stores/settingsStore';

import { hintText, sectionTitle } from './Field';
import { IntegrationCard } from './IntegrationCard';
import { keychainMessageKey } from './keychainMessage';
import { SwitchCatalogGroup } from './SwitchCatalogGroup';

/** Page order of the groups (§5.1); a group with no working service is
 * not drawn — its planned ones sit in «Próximamente». */
const GROUP_ORDER: readonly IntegrationGroup[] = [
  'art',
  'names',
  'achievements',
  'catalogs',
  'updates',
];

/** A link button in the quiet, in-text style of the tab. */
const linkClass = 'h-auto justify-self-start p-0';

/**
 * «Integraciones» (integrations spec §5, §6): every service Coverly talks
 * to on the user's behalf, one card each, grouped as the registry says,
 * then the planned ones as a short list and the tabs this one leans on.
 * Opening it sends nothing to anyone (D9): it reads the cards' state from
 * the back end, which only looks at what is stored.
 */
export function IntegrationsSection() {
  const t = useT();
  const locale = useContext(LocaleContext);
  const order = useSettingsStore((s) => s.settings!.sources.order);
  const catalogOn = useSettingsStore(
    (s) => s.settings!.catalogs.switch.enabled,
  );
  const checkOnLaunch = useSettingsStore(
    (s) => s.settings!.general.check_updates_on_launch,
  );
  const channel = useSettingsStore((s) => s.settings!.general.update_channel);
  const openSettings = useNavStore((s) => s.openSettings);
  const setFold = useNavStore((s) => s.setFold);
  const [views, setViews] = useState<IntegrationView[] | null>(null);
  const [platform, setPlatform] = useState('');
  const [error, setError] = useState<BackendError | null>(null);
  const [catalog, setCatalog] = useState<CatalogStatus | null>(null);
  const opened = useRef(false);

  // The cards follow the settings they show (a source turned on here or
  // in Fuentes changes its tag); reading them never touches the network.
  useEffect(() => {
    let alive = true;
    void getBackend()
      .then((b) => b.integrationsView())
      .then(
        (v) => alive && setViews(v),
        (e: unknown) => alive && setError(toBackendError(e)),
      );
    return () => {
      alive = false;
    };
  }, [order, catalogOn]);

  useEffect(() => {
    let alive = true;
    void getBackend()
      .then((b) => b.appInfo())
      .then(
        (i) => alive && setPlatform(i.platform),
        () => undefined,
      );
    return () => {
      alive = false;
    };
  }, []);

  const viewOf = (id: string) => views?.find((v) => v.id === id);
  const keychainKind =
    views?.find((v) => v.reason?.kind === 'keychain')?.reason?.detail ?? null;
  // Nothing stored and the catalogue off (§6.6). A keychain that could not
  // be read is «we could not look», not «nothing there» (D6).
  const empty =
    views !== null &&
    keychainKind === null &&
    !catalogOn &&
    views.every((v) => v.fields.every((f) => !f.set));
  const ssBuilt = viewOf('screenscraper')?.built === true;

  // An empty tab opens every card once (D26): there is nothing to hide.
  useEffect(() => {
    if (!empty || opened.current) return;
    opened.current = true;
    for (const e of WORKING) setFold(integrationDomId(e.id), true);
  }, [empty, setFold]);

  const onCatalog = useCallback((s: CatalogStatus | null) => setCatalog(s), []);

  if (error)
    return (
      <div className="settings-group">
        <Alert variant="danger">
          {t(errorKey(error.code), { detail: error.detail ?? '' })}
        </Alert>
      </div>
    );

  const groups = GROUP_ORDER.map((g) => ({
    group: g,
    entries: WORKING.filter((e) => e.group === g),
  })).filter((g) => g.entries.length > 0);

  const positionOf = (e: Integration) => {
    const i = order.findIndex((o) => o.id === e.source);
    return i < 0 ? undefined : { n: i + 1, total: order.length };
  };

  const needs = new Intl.ListFormat(locale, {
    style: 'long',
    type: 'conjunction',
  });

  return (
    <div className="settings-group">
      <p className="m-0 text-dense text-cr-text-secondary">
        {t('integrations.intro')}
      </p>
      {keychainKind === 'unavailable' && (
        <Alert variant="danger">
          {t(
            keychainMessageKey('unavailable', platform, (k) => t(k) !== k),
            { detail: '' },
          )}
        </Alert>
      )}
      {empty && (
        <div className="grid gap-2 rounded-lg border border-dashed border-cr-border-strong px-3 py-3">
          <p className="m-0 text-dense text-cr-text">
            {t('integrations.empty')}
            {ssBuilt && <> {t('integrations.emptyScreenscraper')}</>}
          </p>
          <Button
            size="sm"
            variant="primary"
            className="justify-self-start"
            onClick={() =>
              openSettings('integrations', {
                focus: integrationDomId('steamgriddb'),
              })
            }
          >
            <KeyRound aria-hidden />
            {t('integrations.configureService', { name: 'SteamGridDB' })}
          </Button>
        </div>
      )}
      {views === null ? (
        <p role="status" className={hintText}>
          {t('settings.loading')}
        </p>
      ) : (
        groups.map(({ group, entries }) => (
          <section
            key={group}
            aria-labelledby={`integrations-group-${group}`}
            className="grid gap-2"
          >
            <h2 id={`integrations-group-${group}`} className={sectionTitle}>
              {t(`integrations.group.${group}`)}
            </h2>
            {entries.map((e) => {
              const view = viewOf(e.id);
              if (!view) return null;
              if (e.id === 'switch-catalog')
                return (
                  <IntegrationCard
                    key={e.id}
                    entry={e}
                    view={view}
                    platform={platform}
                    onView={setViews}
                    card={catalogCardState(
                      catalogOn ? catalog : { state: 'off' },
                    )}
                  >
                    <SwitchCatalogGroup
                      place="integrations"
                      onStatus={onCatalog}
                    />
                  </IntegrationCard>
                );
              if (e.kind === 'link')
                return (
                  <IntegrationCard
                    key={e.id}
                    entry={e}
                    view={view}
                    platform={platform}
                    onView={setViews}
                    card={updatesCardState(checkOnLaunch)}
                  >
                    <div className="grid gap-1">
                      <p className="m-0 text-dense text-cr-text">
                        {t('integrations.updates.channel', {
                          channel: t(`settings.general.channels.${channel}`),
                        })}
                      </p>
                      <div className="flex flex-wrap gap-x-4 gap-y-1">
                        <Button
                          variant="link"
                          size="sm"
                          data-settings-opener
                          className={linkClass}
                          onClick={() =>
                            openSettings('general', {
                              focus: 'setting-general-update-channel',
                            })
                          }
                        >
                          {t('integrations.updates.change')}
                        </Button>
                        <Button
                          variant="link"
                          size="sm"
                          data-settings-opener
                          className={linkClass}
                          onClick={() =>
                            openSettings('general', {
                              focus: 'setting-general-check-updates-on-launch',
                            })
                          }
                        >
                          {t('integrations.updates.checkOnLaunch')}
                        </Button>
                        <Button
                          variant="link"
                          size="sm"
                          className={linkClass}
                          onClick={() =>
                            void getBackend().then((b) =>
                              b.openLink('releases'),
                            )
                          }
                        >
                          {t('integrations.releases')}
                          <ExternalLink aria-hidden />
                        </Button>
                      </div>
                    </div>
                  </IntegrationCard>
                );
              return (
                <IntegrationCard
                  key={e.id}
                  entry={e}
                  view={view}
                  order={positionOf(e)}
                  platform={platform}
                  onView={setViews}
                />
              );
            })}
          </section>
        ))
      )}
      <section
        aria-labelledby="integrations-group-planned"
        className="grid gap-2"
      >
        <h2 id="integrations-group-planned" className={sectionTitle}>
          {t('integrations.group.planned')}
        </h2>
        <ul
          id="integrations-planned"
          tabIndex={-1}
          className={cn(
            'm-0 grid list-none gap-1.5 rounded-lg p-0 outline-none',
            'data-[arrived=true]:bg-cr-surface',
          )}
          data-setting-row=""
        >
          {PLANNED.map((p) => (
            <li
              key={p.id}
              className="grid grid-cols-[minmax(9rem,max-content)_1fr] gap-x-3 max-[34rem]:grid-cols-1"
            >
              <strong className="text-dense font-medium text-cr-text">
                {p.name}
              </strong>
              <span className="grid">
                <span className="text-dense text-cr-text">
                  {t(`integrations.service.${p.id}.use`)}
                </span>
                <span className={hintText}>
                  {p.needs.length > 0
                    ? needs.format(
                        p.needs.map((n) => t(`integrations.needs.${n}`)),
                      )
                    : t('integrations.needs.none')}
                </span>
              </span>
            </li>
          ))}
        </ul>
        <Button
          variant="link"
          size="sm"
          className={linkClass}
          onClick={() => void getBackend().then((b) => b.openLink('roadmap'))}
        >
          {t('integrations.planned.plan')}
          <ExternalLink aria-hidden />
        </Button>
      </section>
      <nav
        aria-label={t('settings.section.integrations')}
        className="flex flex-wrap gap-x-4 gap-y-1 border-t border-cr-border pt-3"
      >
        <Button
          variant="link"
          size="sm"
          data-settings-opener
          className={linkClass}
          onClick={() => openSettings('network')}
        >
          {t('integrations.links.network')}
        </Button>
        <Button
          variant="link"
          size="sm"
          data-settings-opener
          className={linkClass}
          onClick={() => openSettings('sources')}
        >
          {t('integrations.links.sources')}
        </Button>
        <Button
          variant="link"
          size="sm"
          data-settings-opener
          className={linkClass}
          onClick={() => openSettings('credits')}
        >
          {t('integrations.links.credits')}
        </Button>
      </nav>
    </div>
  );
}
