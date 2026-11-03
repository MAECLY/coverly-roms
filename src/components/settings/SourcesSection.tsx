import { ChevronDown, ChevronUp } from 'lucide-react';
import { useEffect, useState } from 'react';

import { Alert } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';
import { SimpleTooltip } from '@/components/ui/tooltip';
import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import { errorKey, toBackendError, type BackendError } from '@/ipc/errors';
import type { CredentialStatus, SourceId, SourceInfo } from '@/ipc/types';
import { integrationForSource } from '@/lib/integrations';
import { useSettingsStore } from '@/stores/settingsStore';

import { errorText, hintText, useFieldError } from './Field';
import { IntegrationLink } from './IntegrationLink';
import { ReorderAnnouncer, useReorderAnnouncer } from './ReorderAnnouncer';

/**
 * «Fuentes» (integrations D7): the order the fill asks the sources in and
 * each one's on/off switch. Keys and accounts live in «Integraciones»; a
 * source that needs one links to its card.
 */

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
          // Only a source with fields has a card to configure.
          const account = integrationForSource(e.id);
          const configurable = (account?.fields.length ?? 0) > 0;
          const hint =
            info === null
              ? null
              : !built(e.id)
                ? t('settings.sources.notBuilt')
                : !ready(e.id)
                  ? t(
                      account?.kind === 'account'
                        ? 'integrations.missingAccount'
                        : 'integrations.missingKey',
                    )
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
                {configurable && account && (
                  <IntegrationLink id={account.id}>
                    {t('integrations.configureIn')}
                  </IntegrationLink>
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
      {error && (
        <Alert variant="danger">
          {t(errorKey(error.code), { detail: error.detail ?? '' })}
        </Alert>
      )}
    </div>
  );
}
