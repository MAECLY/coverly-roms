import { ChevronDown, ChevronUp, FolderOpen } from 'lucide-react';
import { useState } from 'react';

import { Alert } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group';
import { Switch } from '@/components/ui/switch';
import { SimpleTooltip } from '@/components/ui/tooltip';
import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import { errorKey, toBackendError, type BackendError } from '@/ipc/errors';
import {
  DEDUPE_LEVELS,
  type DedupeActionSetting,
  type DedupeLevel,
  type ProfileInfo,
  type RuleToggle,
} from '@/ipc/types';
import { useNavStore } from '@/stores/navStore';
import { useSettingsStore } from '@/stores/settingsStore';

import {
  errorText,
  Field,
  fieldErrorText,
  hintText,
  MoreInfo,
  sectionTitle,
  settingId,
  SwitchSetting,
} from './Field';
import { ReorderAnnouncer, useReorderAnnouncer } from './ReorderAnnouncer';

const DEFAULT_QUARANTINE = '_Duplicados';

const commas = (s: string) =>
  s
    .split(',')
    .map((x) => x.trim())
    .filter(Boolean);

/** The error text of a dedupe setting; a quarantine inside the ROMs or a
 * system folder says why. */
function useDedupeError(): (path: string) => string | null {
  const t = useT();
  const errors = useSettingsStore((s) => s.errors);
  return (path) => {
    const e = errors[path];
    if (!e) return null;
    if (path === 'dedupe.quarantine_dir' && e.code === 'invalid')
      return t('settings.dedupe.quarantineInside');
    return fieldErrorText(t, e);
  };
}

/**
 * Settings › Duplicados, visible part (spec §5.3): what to do with the
 * copies, where they go and one ROM per game. The Duplicates screen starts
 * from these.
 */
export function DedupeCommon() {
  const t = useT();
  const d = useSettingsStore((s) => s.settings!.dedupe);
  const update = useSettingsStore((s) => s.update);
  const errorOf = useDedupeError();
  const [dir, setDir] = useState<string | null>(null);
  const [pickError, setPickError] = useState<BackendError | null>(null);

  const commitDir = async (value: string) => {
    if (
      (await update({ dedupe: { quarantine_dir: value.trim() || null } }))
        .length === 0
    )
      setDir(null);
  };

  const dirId = settingId('dedupe.quarantine_dir');
  const name = d.quarantine_name?.trim() || DEFAULT_QUARANTINE;

  return (
    <>
      <fieldset className="m-0 border-0 p-0" data-setting-row="">
        <legend
          id="dedupe-action-legend"
          className="mb-1.5 p-0 text-ui font-medium text-cr-text"
        >
          {t('settings.dedupe.action')}
        </legend>
        <RadioGroup
          name="dedupe-action"
          aria-labelledby="dedupe-action-legend"
          className="gap-3"
          value={d.action}
          onValueChange={(v) =>
            void update({ dedupe: { action: v as DedupeActionSetting } })
          }
        >
          {(['quarantine', 'trash'] as const).map((a) => {
            const rid = settingId(`dedupe.action.${a}`);
            return (
              <div
                key={a}
                className="grid grid-cols-[auto_1fr] items-start gap-x-2.5 gap-y-0.5"
              >
                <RadioGroupItem id={rid} value={a} className="mt-0.5" />
                <Label htmlFor={rid} className="font-normal">
                  {t(
                    a === 'trash'
                      ? 'settings.dedupe.actionTrash'
                      : 'settings.dedupe.actionQuarantine',
                  )}
                </Label>
                {a === 'trash' && (
                  <p className={`${hintText} col-start-2`}>
                    {t('settings.dedupe.actionTrashHint')}
                  </p>
                )}
              </div>
            );
          })}
        </RadioGroup>
      </fieldset>

      <Field
        id={dirId}
        label={t('settings.dedupe.quarantine')}
        hint={t('settings.dedupe.quarantineHint', { folder: name })}
        more={t('settings.dedupe.quarantineDetails')}
        error={errorOf('dedupe.quarantine_dir')}
      >
        <Input
          name={dirId}
          mono
          className="max-w-md"
          placeholder={t('settings.dedupe.quarantineDefault', { name })}
          value={dir ?? d.quarantine_dir ?? ''}
          onChange={(e) => setDir(e.target.value)}
          onBlur={() => dir !== null && void commitDir(dir)}
        />
      </Field>
      <Button
        className="justify-self-start"
        onClick={async () => {
          setPickError(null);
          let picked: string | null;
          try {
            picked = await (await getBackend()).pickFolder();
          } catch (e) {
            setPickError(toBackendError(e));
            return;
          }
          if (picked) await commitDir(picked);
        }}
      >
        <FolderOpen aria-hidden />
        {t('settings.dedupe.choose')}
      </Button>
      {pickError && (
        <Alert variant="danger">
          {t(errorKey(pickError.code), { detail: pickError.detail ?? '' })}
        </Alert>
      )}

      <SwitchSetting
        id={settingId('dedupe.one_game_one_rom')}
        name="dedupe-one-game-one-rom"
        label={t('settings.dedupe.oneGameOneRom')}
        hint={t('settings.dedupe.oneGameOneRomHint')}
        more={t('settings.dedupe.oneGameOneRomDetails')}
        checked={d.one_game_one_rom}
        onCheckedChange={(on) =>
          void update({ dedupe: { one_game_one_rom: on } })
        }
      />
    </>
  );
}

/**
 * Settings › Duplicados, folded part (spec §5.3): what to look for, the
 * keeper rules in the order they are weighed, the format order per profile
 * and the systems left out; the plan reads these when it looks again.
 * Region order is Matching's; the organise step is Organise's (D15).
 */
export function DedupeAdvanced({
  profiles = [],
}: {
  /** Layouts by id, to name each format order. */
  profiles?: ProfileInfo[];
}) {
  const t = useT();
  const d = useSettingsStore((s) => s.settings!.dedupe);
  const update = useSettingsStore((s) => s.update);
  const errorOf = useDedupeError();
  const announcer = useReorderAnnouncer();
  const [exclude, setExclude] = useState<string | null>(null);
  const [orders, setOrders] = useState<Record<string, string>>({});

  const ruleName = (r: RuleToggle['rule']) => t(`settings.dedupe.rule.${r}`);

  const setLevel = (level: DedupeLevel, on: boolean) =>
    void update({
      dedupe: {
        levels: DEDUPE_LEVELS.filter((l) =>
          l === level ? on : d.levels.includes(l),
        ),
      },
    });
  const setRules = (keep_rules: RuleToggle[]) =>
    void update({ dedupe: { keep_rules } });
  const move = (i: number, by: -1 | 1) => {
    const next = [...d.keep_rules];
    const [x] = next.splice(i, 1);
    next.splice(i + by, 0, x!);
    setRules(next);
    announcer.moved(ruleName(x!.rule), i + by + 1, next.length);
  };

  const rulesId = 'setting-dedupe-keep-rules';
  const levelsError = errorOf('dedupe.levels');
  const rulesError = errorOf('dedupe.keep_rules');
  const orderRows = Object.entries(d.container_order).flatMap(
    ([profile, kinds]) =>
      Object.entries(kinds).map(([kind, list]) => ({ profile, kind, list })),
  );

  return (
    <>
      <p className={hintText}>{t('settings.dedupe.defaultsNote')}</p>

      <div
        className="grid gap-2"
        role="group"
        aria-labelledby="dedupe-levels"
        data-setting-row=""
      >
        <p id="dedupe-levels" className="m-0 text-ui font-medium text-cr-text">
          {t('settings.dedupe.levels')}
        </p>
        {DEDUPE_LEVELS.map((level) => (
          <SwitchSetting
            key={level}
            id={settingId(`dedupe.levels.${level}`)}
            name={`dedupe-level-${level}`}
            label={t(`dedupe.level.${level}`)}
            checked={d.levels.includes(level)}
            onCheckedChange={(on) => setLevel(level, on)}
          />
        ))}
        {levelsError && (
          <p role="alert" className={errorText}>
            {levelsError}
          </p>
        )}
      </div>

      <div className="grid gap-2">
        <h3 className={sectionTitle} id="dedupe-rules">
          {t('settings.dedupe.rules')}
        </h3>
        <p className={hintText}>{t('settings.dedupe.rulesHint')}</p>
        <MoreInfo id={rulesId}>{t('settings.dedupe.rulesDetails')}</MoreInfo>
        {/* The list is the landing target (SP-1): the first row's own
            buttons would be an arbitrary place to land. */}
        <ol
          id={rulesId}
          tabIndex={-1}
          data-setting-row=""
          className="m-0 grid list-decimal gap-1 pl-6 marker:text-caption marker:text-cr-text-secondary"
          aria-labelledby="dedupe-rules"
        >
          {d.keep_rules.map((r, i) => {
            const label = ruleName(r.rule);
            const up = t('settings.dedupe.moveUp', { rule: label });
            const down = t('settings.dedupe.moveDown', { rule: label });
            return (
              <li key={r.rule} className="pl-1">
                <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
                  <Switch
                    name={`dedupe-rule-${r.rule}`}
                    aria-label={t('settings.dedupe.use', { rule: label })}
                    checked={r.on}
                    onCheckedChange={(on) =>
                      setRules(
                        d.keep_rules.map((x) =>
                          x.rule === r.rule ? { ...x, on } : x,
                        ),
                      )
                    }
                  />
                  <span className="min-w-0 flex-1 text-ui">
                    {label}
                    {r.rule === 'region' && (
                      <>
                        {' · '}
                        <Button
                          variant="link"
                          size="sm"
                          className="h-auto shrink whitespace-normal p-0 text-left align-baseline"
                          onClick={() =>
                            useNavStore.getState().setSection('matching')
                          }
                        >
                          {t('settings.dedupe.regionOrder')}
                        </Button>
                      </>
                    )}
                  </span>
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
                      disabled={i === d.keep_rules.length - 1}
                      aria-label={down}
                      onClick={() => move(i, 1)}
                    >
                      <ChevronDown aria-hidden />
                    </Button>
                  </SimpleTooltip>
                </div>
              </li>
            );
          })}
        </ol>
        <ReorderAnnouncer message={announcer.message} />
        {rulesError && (
          <p role="alert" className={errorText}>
            {rulesError}
          </p>
        )}
      </div>

      {/* A plain sub-group: never a fold inside the fold (spec §5.3). */}
      <div
        role="group"
        id="setting-dedupe-container-order"
        tabIndex={-1}
        aria-labelledby="dedupe-container-order"
        data-setting-row=""
        className="grid gap-3"
      >
        <h3 className={sectionTitle} id="dedupe-container-order">
          {t('settings.dedupe.containerOrder')}
        </h3>
        <p className={hintText}>{t('settings.dedupe.containerOrderHint')}</p>
        {orderRows.map(({ profile, kind, list }) => {
          const key = `${profile}/${kind}`;
          const path = `dedupe.container_order.${profile}`;
          const id = settingId(`${path}.${kind}`);
          return (
            <Field
              key={key}
              id={id}
              label={t('settings.dedupe.containerKey', {
                profile:
                  profile === '*'
                    ? t('settings.dedupe.containerAll')
                    : (profiles.find((p) => p.profile.id === profile)?.profile
                        .name ?? profile),
                kind: t(`settings.dedupe.containerKind.${kind}`) || kind,
              })}
              error={errorOf(path)}
            >
              <Input
                name={id}
                mono
                className="max-w-md"
                value={orders[key] ?? list.join(', ')}
                onChange={(e) =>
                  setOrders((o) => ({ ...o, [key]: e.target.value }))
                }
                onBlur={async () => {
                  const draft = orders[key];
                  if (draft === undefined) return;
                  const next = {
                    ...d.container_order,
                    [profile]: {
                      ...d.container_order[profile],
                      [kind]: commas(draft).map((c) => c.toLowerCase()),
                    },
                  };
                  const errs = await update({
                    dedupe: { container_order: next },
                  });
                  if (errs.length === 0)
                    setOrders(({ [key]: _done, ...rest }) => rest);
                }}
              />
            </Field>
          );
        })}
      </div>

      <Field
        id={settingId('dedupe.exclude_systems')}
        label={t('settings.dedupe.exclude')}
        hint={t('settings.dedupe.excludeHint')}
        error={errorOf('dedupe.exclude_systems')}
      >
        <Input
          name="dedupe-exclude-systems"
          mono
          className="max-w-md"
          value={exclude ?? d.exclude_systems.join(', ')}
          onChange={(e) => setExclude(e.target.value)}
          onBlur={async () => {
            if (exclude === null) return;
            const errs = await update({
              dedupe: { exclude_systems: commas(exclude) },
            });
            if (errs.length === 0) setExclude(null);
          }}
        />
      </Field>

      <p className={hintText}>
        {t('settings.dedupe.organizeStepWhere')}{' '}
        <Button
          variant="link"
          size="sm"
          className="h-auto p-0 align-baseline"
          onClick={() =>
            useNavStore.getState().openSettings('organize', {
              focus: settingId('dedupe.organize_step'),
            })
          }
        >
          {t('settings.dedupe.organizeStepLink')}
        </Button>
      </p>
    </>
  );
}
