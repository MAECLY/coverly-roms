import {
  ChevronDown,
  Database,
  ExternalLink,
  KeyRound,
  RefreshCw,
  UserRound,
  type LucideIcon,
} from 'lucide-react';
import { useContext, useEffect, useRef, useState, type ReactNode } from 'react';

import { Button } from '@/components/ui/button';
import { Collapsible, CollapsibleContent } from '@/components/ui/collapsible';
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';
import { LocaleContext } from '@/i18n/core';
import { useRichT } from '@/i18n/rich';
import { useT, type TFunction } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import { errorKey, toBackendError, type BackendError } from '@/ipc/errors';
import type {
  CredentialKey,
  IntegrationView,
  KeyCheck,
  KeychainKind,
} from '@/ipc/types';
import { formatRelative } from '@/lib/format';
import {
  integrationDomId,
  integrationName,
  type CardState,
  type Integration,
  type IntegrationKind,
} from '@/lib/integrations';
import { cn, focusRing } from '@/lib/utils';
import { useNavStore } from '@/stores/navStore';
import { useSettingsStore } from '@/stores/settingsStore';

import { errorText, hintText, settingId, useFieldError } from './Field';
import { IntegrationStatusTag } from './IntegrationStatusTag';
import { keychainMessageKey } from './keychainMessage';
import { SecretField } from './SecretField';

/** An app icon per kind: no service logos (D25). */
const ICON: Record<IntegrationKind, LucideIcon> = {
  'api-key': KeyRound,
  account: UserRound,
  dataset: Database,
  link: RefreshCw,
};

/** Cards the user opened or closed this session: a problem opens a card
 * (D26) only until the user decides otherwise. */
const touched = new Set<string>();

/** Tests: forget which cards the user folded. */
// eslint-disable-next-line react-refresh/only-export-components -- test reset beside the component it belongs to
export function resetIntegrationCardMemory() {
  touched.clear();
}

/** The per-OS copy of a keychain error (D16, §7.3). */
function keychainText(t: TFunction, kind: KeychainKind, platform: string) {
  const key = keychainMessageKey(kind, platform, (k) => t(k) !== k);
  return t(key, { detail: '' });
}

/** The copy of a failed call: a keychain error says what to do here. */
function errorMessage(t: TFunction, e: BackendError, platform: string) {
  if (e.code.startsWith('keychain-') && e.code !== 'keychain-failed')
    return keychainText(t, e.code.slice(9) as KeychainKind, platform);
  return t(errorKey(e.code), { detail: e.detail ?? '' });
}

type Result =
  { kind: 'check'; verdict: KeyCheck } | { kind: 'error'; error: BackendError };

/**
 * One service of «Integraciones» (spec §5.2, §6.1–§6.4, D8–D14, D22,
 * D26): a row whose header says what it is and how it stands, and whose
 * body — always mounted, `hidden` while folded, so deep links and the
 * search land inside it — holds its switch, fields, test, quota, help,
 * privacy and credit. Values never come back from the back end: a field
 * only says it is stored.
 */
export function IntegrationCard({
  entry,
  view,
  order,
  platform,
  onView,
  card,
  children,
}: {
  entry: Integration;
  view: IntegrationView;
  /** Position in `sources.order` and its length, for «Fuentes: posición n
   * de m». */
  order?: { n: number; total: number } | undefined;
  platform: string;
  /** A save, test or forget brings a fresh view of every card. */
  onView(next: IntegrationView[]): void;
  /** The catalogue's state mapped from its own status (IP-1). */
  card?: CardState | undefined;
  /** The Switch catalogue body (its status, actions and fold). */
  children?: ReactNode;
}) {
  const t = useT();
  const tr = useRichT();
  const locale = useContext(LocaleContext);
  const id = entry.id;
  const domId = integrationDomId(id);
  const fold = domId;
  const bodyId = `${domId}-body`;
  const useLineId = `${domId}-use`;
  const stateId = `${domId}-state`;
  const unavailableId = `${domId}-unavailable`;
  const open = useNavStore((s) => s.openFolds.includes(fold));
  const setFold = useNavStore((s) => s.setFold);
  const openSettings = useNavStore((s) => s.openSettings);
  const name = integrationName(entry, t);
  const state = card ?? { state: view.state };
  const Icon = ICON[entry.kind];

  // A problem is never hidden (D26), unless the user folded it away.
  const problem = state.state === 'error' || state.state === 'quota';
  useEffect(() => {
    if (problem && !touched.has(fold)) setFold(fold, true);
  }, [problem, fold, setFold]);

  /** Bumped by every save, test and forget: an answer whose number is no
   * longer current is about a key that has changed since. */
  const generation = useRef(0);
  const [testing, setTesting] = useState(false);
  const [result, setResult] = useState<Result | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [forgetError, setForgetError] = useState<BackendError | null>(null);
  const forgetRef = useRef<HTMLButtonElement>(null);
  const cancelRef = useRef<HTMLButtonElement>(null);
  const headerRef = useRef<HTMLButtonElement>(null);
  const refocusForget = useRef(false);
  const confirmRef = useRef<HTMLDivElement>(null);

  // Escape cancels the confirm and stops at the card (Settings has no
  // Escape of its own). A native listener, so nothing above sees it.
  useEffect(() => {
    const group = confirmRef.current;
    if (!confirming || !group) return;
    const onKey = (e: globalThis.KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      e.stopPropagation();
      e.preventDefault();
      refocusForget.current = true;
      setConfirming(false);
    };
    group.addEventListener('keydown', onKey);
    return () => group.removeEventListener('keydown', onKey);
  }, [confirming]);

  useEffect(() => {
    if (confirming) cancelRef.current?.focus();
    else if (refocusForget.current) {
      refocusForget.current = false;
      forgetRef.current?.focus();
    }
  }, [confirming]);

  const fresh = () => {
    generation.current += 1;
    setTesting(false);
    setResult(null);
  };

  async function refresh() {
    try {
      onView(await (await getBackend()).integrationsView());
    } catch {
      // The card keeps what it shows; the next action reports problems.
    }
  }

  async function save(key: CredentialKey, value: string) {
    fresh();
    // A failure is shown by the field itself.
    await (await getBackend()).setCredential(key, value);
    await refresh();
  }

  async function test() {
    const mine = ++generation.current;
    const current = () => generation.current === mine;
    setTesting(true);
    setResult(null);
    try {
      const r = await (await getBackend()).testIntegration(id);
      if (!current()) return;
      setResult({ kind: 'check', verdict: r.verdict });
      await refresh();
    } catch (e) {
      if (current()) setResult({ kind: 'error', error: toBackendError(e) });
    } finally {
      if (current()) setTesting(false);
    }
  }

  async function forget() {
    fresh();
    setForgetError(null);
    try {
      onView(await (await getBackend()).forgetIntegration(id));
      setConfirming(false);
      headerRef.current?.focus();
    } catch (e) {
      setForgetError(toBackendError(e));
      setConfirming(false);
    }
  }

  function cancel() {
    refocusForget.current = true;
    setConfirming(false);
  }

  const anySet = view.fields.some((f) => f.set);
  const allSet = entry.fields.every(
    (f) => !f.required || view.fields.some((v) => v.key === f.key && v.set),
  );
  const keychainKind =
    view.reason?.kind === 'keychain' ? view.reason.detail : null;
  const line = stateLine({
    t,
    tr,
    entry,
    view,
    state: state.state,
    allSet,
    platform,
    locale,
  });

  const toggle = entry.source ? (
    <SourceToggle
      entry={entry}
      domId={domId}
      enabled={view.enabled}
      // An enabled source can always be turned off (D12).
      usable={view.enabled || (view.built && allSet)}
      describedBy={line ? stateId : undefined}
      order={order}
      onOrder={() =>
        openSettings('sources', { focus: `source-${entry.source}` })
      }
    />
  ) : null;

  const quota = view.quota;
  const number = new Intl.NumberFormat(locale);
  const howTo =
    entry.kind === 'account'
      ? 'integrations.howToAccount'
      : 'integrations.howTo';
  const steps = `integrations.service.${id}.steps`;
  const unlocks = `integrations.service.${id}.unlocks`;
  const privacy = `integrations.service.${id}.privacy`;

  return (
    <Collapsible open={open} asChild>
      <article
        data-setting-row=""
        data-advanced-fold={fold}
        aria-labelledby={domId}
        className={cn(
          'grid rounded-lg border border-cr-border',
          open ? 'bg-cr-surface-raised' : 'bg-cr-surface',
        )}
      >
        <h3 className="m-0">
          <button
            ref={headerRef}
            type="button"
            id={domId}
            aria-expanded={open}
            aria-controls={bodyId}
            aria-describedby={useLineId}
            onClick={() => {
              touched.add(fold);
              setFold(fold, !open);
            }}
            className={cn(
              'group flex w-full flex-wrap items-center gap-x-2.5 gap-y-1 rounded-lg px-3 pt-2.5 text-left text-dense font-medium text-cr-text',
              focusRing,
            )}
          >
            <Icon aria-hidden className="size-4 shrink-0 text-cr-primary" />
            <span>
              {name}
              <span className="sr-only">,</span>
            </span>{' '}
            <IntegrationStatusTag state={state} />
            <ChevronDown
              aria-hidden
              className={cn(
                'ml-auto size-4 shrink-0 text-cr-text-secondary transition-transform duration-fast ease-standard motion-reduce:transition-none',
                open && 'rotate-180',
              )}
            />
          </button>
        </h3>
        <p id={useLineId} className={cn(hintText, 'px-3 pb-2.5 pl-[2.375rem]')}>
          {t(`integrations.service.${id}.use`)}
        </p>
        {/* forceMount keeps Radix from unmounting it (D14): hide it
            ourselves, and keep `grid` from overriding `hidden`. */}
        <CollapsibleContent
          forceMount
          id={bodyId}
          hidden={!open}
          className="grid gap-3 border-t border-cr-border px-3 py-3 data-[state=closed]:hidden"
        >
          {line && (
            <p
              id={stateId}
              className={cn(
                hintText,
                state.state === 'error' && 'font-medium text-cr-danger',
                state.state === 'quota' && 'font-medium text-cr-warning',
                state.state === 'not-configured' && 'text-cr-warning',
              )}
            >
              {line}
            </p>
          )}
          {t(unlocks) !== unlocks && <p className={hintText}>{t(unlocks)}</p>}
          {toggle}
          {entry.fields.map((f) => (
            <SecretField
              key={f.key}
              k={f.key}
              input={f.input}
              set={view.fields.some((v) => v.key === f.key && v.set)}
              // A keychain that is not there cannot store anything; a
              // locked or refusing one may work on the next try.
              disabled={keychainKind === 'unavailable'}
              onSave={(value) => save(f.key, value)}
            />
          ))}
          {(entry.test || (entry.fields.length > 0 && anySet)) &&
            (confirming ? (
              <div
                role="group"
                aria-labelledby={`${domId}-forget-question`}
                className="grid gap-2"
                ref={confirmRef}
              >
                <p
                  id={`${domId}-forget-question`}
                  className="m-0 text-dense text-cr-text"
                >
                  {t('integrations.forgetConfirm', { name })}
                </p>
                <div className="flex flex-wrap gap-2">
                  <Button
                    size="sm"
                    variant="destructive"
                    onClick={() => void forget()}
                  >
                    {t('integrations.forgetYes')}
                  </Button>
                  <Button
                    ref={cancelRef}
                    size="sm"
                    variant="outline"
                    onClick={cancel}
                  >
                    {t('integrations.cancel')}
                  </Button>
                </div>
              </div>
            ) : (
              <div className="flex flex-wrap gap-2">
                {entry.test && (
                  <Button
                    size="sm"
                    variant="outline"
                    disabled={!view.built || !anySet}
                    loading={testing}
                    aria-describedby={view.built ? undefined : unavailableId}
                    onClick={() => void test()}
                  >
                    {testing
                      ? t('integrations.testing')
                      : t('integrations.test')}
                  </Button>
                )}
                {entry.fields.length > 0 && anySet && (
                  <Button
                    ref={forgetRef}
                    size="sm"
                    variant="ghost"
                    onClick={() => {
                      setForgetError(null);
                      setConfirming(true);
                    }}
                  >
                    {t('integrations.forget')}
                  </Button>
                )}
              </div>
            ))}
          {!view.built && (
            <p id={unavailableId} hidden>
              {t('integrations.ssUnavailable')}
            </p>
          )}
          {forgetError && (
            <p role="alert" className={errorText}>
              {errorMessage(t, forgetError, platform)}
            </p>
          )}
          {entry.test && (
            // Mounted empty with the card: a region inserted together with
            // its text is not announced (§6.4).
            <p
              role="status"
              aria-live="polite"
              className={cn(
                'm-0 text-caption font-medium empty:hidden',
                result?.kind === 'check' && result.verdict === 'ok'
                  ? 'text-cr-success'
                  : 'text-cr-danger',
              )}
            >
              {result?.kind === 'check'
                ? t(`integrations.check.${result.verdict}`)
                : result?.kind === 'error'
                  ? errorMessage(t, result.error, platform)
                  : null}
            </p>
          )}
          {quota && (
            <p
              data-testid={`${domId}-quota`}
              className="m-0 flex flex-wrap gap-x-3 gap-y-0.5 text-caption text-cr-text-secondary"
            >
              <span>
                {tr(
                  'integrations.quotaToday',
                  {
                    used: number.format(quota.requests_today),
                    max: number.format(quota.max_requests_per_day),
                  },
                  ['used', 'max'],
                )}
              </span>
              <span>
                {tr(
                  'integrations.threads',
                  { n: number.format(quota.max_threads) },
                  ['n'],
                )}
              </span>
              {view.quota_at !== undefined && (
                <span>
                  {t('integrations.seen', {
                    time: formatRelative(view.quota_at * 1000, locale),
                  })}
                </span>
              )}
            </p>
          )}
          {children}
          {entry.help_url && (
            <div className="grid gap-0.5">
              <Button
                variant="link"
                size="sm"
                className="h-auto justify-self-start p-0"
                aria-describedby={`${domId}-steps`}
                onClick={() =>
                  void getBackend().then((b) => b.openIntegrationHelp(id))
                }
              >
                {t(howTo)}
                <ExternalLink aria-hidden />
              </Button>
              {t(steps) !== steps && (
                <p id={`${domId}-steps`} className={hintText}>
                  {t(steps)}
                </p>
              )}
            </div>
          )}
          {t(privacy) !== privacy && (
            <dl className="m-0 grid gap-0.5">
              <dt className="text-caption font-medium text-cr-text">
                {t('integrations.privacy')}
              </dt>
              <dd className={cn(hintText, 'ml-0')}>{t(privacy)}</dd>
            </dl>
          )}
          {entry.credits[0] && (
            <Button
              variant="link"
              size="sm"
              data-settings-opener
              className="h-auto justify-self-start p-0"
              onClick={() =>
                openSettings('credits', {
                  focus: `credit-${entry.credits[0]}`,
                })
              }
            >
              {t('integrations.credit')}
            </Button>
          )}
        </CollapsibleContent>
      </article>
    </Collapsible>
  );
}

/** The «Activado» switch: the same `sources.order` patch Fuentes writes
 * (D12), its refusal shown beside it, and the source's place in the order. */
function SourceToggle({
  entry,
  domId,
  enabled,
  usable,
  describedBy,
  order,
  onOrder,
}: {
  entry: Integration;
  domId: string;
  enabled: boolean;
  usable: boolean;
  describedBy?: string | undefined;
  order?: { n: number; total: number } | undefined;
  onOrder(): void;
}) {
  const t = useT();
  const tr = useRichT();
  const sources = useSettingsStore((s) => s.settings?.sources.order);
  const update = useSettingsStore((s) => s.update);
  const orderError = useFieldError('sources.order');
  const id = settingId(entry.setting ?? `integrations.${entry.id}.enabled`);
  const errorId = `${domId}-order-error`;
  const checked =
    sources?.find((e) => e.id === entry.source)?.enabled ?? enabled;
  const description =
    [describedBy, orderError && errorId].filter(Boolean).join(' ') || undefined;
  return (
    <div className="grid gap-1">
      <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1">
        <Switch
          id={id}
          name={`integration-${entry.id}-enabled`}
          checked={checked}
          disabled={!usable || !sources}
          aria-describedby={description}
          aria-invalid={orderError ? true : undefined}
          onCheckedChange={(on) =>
            sources &&
            void update({
              sources: {
                order: sources.map((x) =>
                  x.id === entry.source ? { ...x, enabled: on } : x,
                ),
              },
            })
          }
        />
        <Label htmlFor={id} className="font-normal">
          {t('integrations.enabled')}
        </Label>
        {order && (
          <Button
            variant="link"
            size="sm"
            data-settings-opener
            className="ml-auto h-auto p-0 font-normal"
            onClick={onOrder}
          >
            {/* One flex item: the button's gap would split the sentence. */}
            <span>
              {tr('integrations.order', { n: order.n, total: order.total }, [
                'n',
                'total',
              ])}
            </span>
          </Button>
        )}
      </div>
      {orderError && (
        <p id={errorId} role="alert" className={errorText}>
          {orderError}
        </p>
      )}
    </div>
  );
}

/** The line under the header that says why the card stands where it does
 * and what to do (§6.3). */
function stateLine({
  t,
  tr,
  entry,
  view,
  state,
  allSet,
  platform,
  locale,
}: {
  t: TFunction;
  tr: ReturnType<typeof useRichT>;
  entry: Integration;
  view: IntegrationView;
  state: IntegrationView['state'];
  allSet: boolean;
  platform: string;
  locale: string;
}): ReactNode {
  const credentialed = entry.fields.length > 0;
  const missing = () => {
    const unset = entry.fields.filter(
      (f) => !view.fields.some((v) => v.key === f.key && v.set),
    );
    if (entry.kind === 'account' && unset.length === entry.fields.length)
      return t('integrations.stateLine.missing.account');
    const first = unset[0];
    return first ? t(`integrations.stateLine.missing.${first.input}`) : null;
  };
  switch (state) {
    case 'off':
      // Off and not ready: why it cannot be turned on comes first.
      if (!view.built) return t('integrations.ssUnavailable');
      if (credentialed && !allSet) return missing();
      if (entry.powers.includes('fill'))
        return t('integrations.stateLine.off.fill');
      if (entry.powers.includes('organize-names'))
        return t('integrations.stateLine.off.organize');
      return null;
    case 'unavailable':
      return t('integrations.ssUnavailable');
    case 'not-configured':
      if (view.reason?.kind === 'missing') {
        if (entry.kind === 'account') return missing();
        return t(`integrations.stateLine.missing.${view.reason.detail}`);
      }
      return missing();
    case 'saved':
      return credentialed ? t('integrations.stateLine.saved') : null;
    case 'connected':
      return credentialed && view.checked_at !== undefined
        ? t('integrations.stateLine.connected', {
            time: formatRelative(view.checked_at * 1000, locale),
          })
        : null;
    case 'error': {
      const r = view.reason;
      if (r?.kind === 'keychain') return keychainText(t, r.detail, platform);
      if (r?.kind === 'check') return t(`integrations.check.${r.detail}`);
      return null;
    }
    case 'quota': {
      const q = view.quota;
      if (!q) return null;
      const n = new Intl.NumberFormat(locale);
      return tr(
        'integrations.stateLine.quota',
        {
          used: n.format(q.requests_today),
          max: n.format(q.max_requests_per_day),
        },
        ['used', 'max'],
      );
    }
  }
}
