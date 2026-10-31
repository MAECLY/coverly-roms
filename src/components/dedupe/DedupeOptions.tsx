/**
 * The options strip above the groups (spec §11.3, 3): which levels to look
 * for (a multi toggle with live counts), what to do with the copies (set
 * aside in a named folder, or the Trash with its caveats) and one ROM per
 * game. A change that moves the count says by how much next to the
 * control ("+37 archivos") and in the live region, in the same frame: a
 * count never grows silently. A `Kbd` legend lists the group keys.
 */
import { useId, useState } from 'react';

import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Kbd, KbdGroup } from '@/components/ui/kbd';
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group';
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group';
import { useT } from '@/i18n/useT';
import {
  DEDUPE_LEVELS,
  type DedupeActionSetting,
  type DedupeLevel,
  type DedupePlanView,
  type DedupeViewOptions,
} from '@/ipc/dedupeTypes';
import { extrasFor, fileName, levelCounts } from '@/lib/dedupe';
import { modKeyLabel } from '@/lib/keyboard';
import { revealOs } from '@/lib/reveal';

import './dedupe.css';

export interface DedupeOptionsProps {
  plan: DedupePlanView;
  options: DedupeViewOptions;
  action: DedupeActionSetting;
  onOptionsChange(next: DedupeViewOptions): void;
  onActionChange(action: DedupeActionSetting): void;
  /** "Cambiar…": pick another quarantine folder. */
  onChangeFolder(): void;
  /** The library's volume has no Trash: the option is off, with why. */
  trashUnavailable?: boolean | undefined;
  /** An answer is pending: the controls wait for it. */
  disabled?: boolean | undefined;
}

type Control = 'levels' | 'oneGameOneRom';

interface Delta {
  control: Control;
  delta: number;
  total: number;
}

/** Splits a message around one placeholder, to style the value. */
function around(
  message: (value: string) => string,
): [before: string, after: string] {
  const MARK = '\u0000';
  const [before = '', after = ''] = message(MARK).split(MARK);
  return [before, after];
}

export function DedupeOptions({
  plan,
  options,
  action,
  onOptionsChange,
  onActionChange,
  onChangeFolder,
  trashUnavailable = false,
  disabled = false,
}: DedupeOptionsProps) {
  const t = useT();
  const uid = useId();
  const [delta, setDelta] = useState<Delta | null>(null);
  const counts = levelCounts(plan);
  const folder = `${fileName(plan.quarantine)}/`;
  const versionsOn = options.levels.includes('version');

  function change(control: Control, next: DedupeViewOptions) {
    const total = extrasFor(plan, next);
    setDelta({ control, delta: total - extrasFor(plan, options), total });
    onOptionsChange(next);
  }

  const deltaText = (d: Delta) =>
    d.delta > 0
      ? t('dedupe.delta', { count: d.delta })
      : t('dedupe.deltaLess', { count: -d.delta });

  const deltaBy = (control: Control) =>
    delta && delta.control === control && delta.delta !== 0 ? (
      <span className="dup-options__delta dup-mono">{deltaText(delta)}</span>
    ) : null;

  const [quarantineBefore, quarantineAfter] = around((folder) =>
    t('dedupe.action.quarantine', { folder }),
  );
  const levelsLabel = `${uid}-levels`;
  const actionLabel = `${uid}-action`;

  return (
    <section className="dup-options" aria-label={t('dedupe.levelsLabel')}>
      <div className="dup-options__row" data-option="levels">
        <span id={levelsLabel} className="dup-options__label">
          {t('dedupe.levelsLabel')}
        </span>
        <div className="dup-options__controls">
          <ToggleGroup
            type="multiple"
            variant="outline"
            size="sm"
            aria-labelledby={levelsLabel}
            value={options.levels}
            disabled={disabled}
            onValueChange={(v) =>
              change('levels', {
                ...options,
                // Keep the canonical order, whatever order you clicked in.
                levels: DEDUPE_LEVELS.filter((l) =>
                  (v as DedupeLevel[]).includes(l),
                ),
              })
            }
            className="flex-wrap"
          >
            {DEDUPE_LEVELS.map((level) => (
              <ToggleGroupItem key={level} value={level}>
                {t(`dedupe.level.${level}`)}{' '}
                <span className="dup-mono">{counts[level]}</span>
                {level === 'version' && !options.one_game_one_rom && (
                  <>
                    {' '}
                    <span className="dup-options__muted">
                      {t('dedupe.reportOnly')}
                    </span>
                  </>
                )}
              </ToggleGroupItem>
            ))}
          </ToggleGroup>
          {deltaBy('levels')}
        </div>
      </div>

      <div className="dup-options__row" data-option="action">
        <span id={actionLabel} className="dup-options__label">
          {t('dedupe.action.label')}
        </span>
        <div className="dup-options__controls dup-options__controls--stack">
          <RadioGroup
            aria-labelledby={actionLabel}
            value={action}
            disabled={disabled}
            onValueChange={(v) => onActionChange(v as DedupeActionSetting)}
            className="dup-options__actions"
          >
            <div className="dup-options__choice">
              <RadioGroupItem
                id={`${uid}-quarantine`}
                value="quarantine"
                aria-label={t('dedupe.action.quarantine', { folder })}
              />
              <label htmlFor={`${uid}-quarantine`}>
                {quarantineBefore}
                <span className="dup-mono">{folder}</span>
                {quarantineAfter}
              </label>
              <Button
                variant="link"
                size="sm"
                disabled={disabled}
                onClick={onChangeFolder}
              >
                {t('dedupe.action.change')}
              </Button>
            </div>
            <div className="dup-options__choice">
              <RadioGroupItem
                id={`${uid}-trash`}
                value="trash"
                disabled={trashUnavailable && action !== 'trash'}
                aria-describedby={
                  trashUnavailable ? `${uid}-no-trash` : undefined
                }
              />
              <label htmlFor={`${uid}-trash`}>{t('dedupe.action.trash')}</label>
            </div>
          </RadioGroup>
          {trashUnavailable && (
            <p id={`${uid}-no-trash`} className="dup-options__muted">
              {t('dedupe.trash.unavailable')}
            </p>
          )}
          {action === 'trash' && (
            <div className="dup-options__caveats">
              <p>{t('dedupe.trash.caveat')}</p>
              {plan.removable && <p>{t('dedupe.trash.cardNote')}</p>}
            </div>
          )}
        </div>
      </div>

      <div className="dup-options__row" data-option="oneGameOneRom">
        <span className="dup-options__label" aria-hidden />
        <div className="dup-options__controls">
          <label className="dup-options__check">
            <Checkbox
              checked={options.one_game_one_rom}
              disabled={disabled || !versionsOn}
              onCheckedChange={(c) =>
                change('oneGameOneRom', {
                  ...options,
                  one_game_one_rom: c === true,
                })
              }
            />
            {t('dedupe.oneGameOneRom')}
          </label>
          {deltaBy('oneGameOneRom')}
        </div>
      </div>

      <ul className="dup-options__keys" aria-label={t('dedupe.keys.label')}>
        <li>
          <KbdGroup>
            <Kbd>↑</Kbd>
            <Kbd>↓</Kbd>
          </KbdGroup>
          {t('dedupe.keys.pick')}
        </li>
        <li>
          <Kbd>Tab</Kbd>
          {t('dedupe.keys.next')}
        </li>
        <li>
          <KbdGroup>
            <Kbd>J</Kbd>
            <Kbd>K</Kbd>
          </KbdGroup>
          {t('dedupe.keys.game')}
        </li>
        <li>
          <Kbd>1–9</Kbd>
          {t('dedupe.keys.number')}
        </li>
        <li>
          <Kbd>X</Kbd>
          {t('dedupe.skipGroup')}
        </li>
        <li>
          <KbdGroup>
            {(revealOs() === 'mac'
              ? ['⌥', '⌘', 'R']
              : ['Shift', 'Alt', 'R']
            ).map((k) => (
              <Kbd key={k}>{k}</Kbd>
            ))}
          </KbdGroup>
          {t('dedupe.keys.reveal')}
        </li>
        <li>
          <KbdGroup>
            <Kbd>{modKeyLabel()}</Kbd>
            <Kbd>{modKeyLabel() === '⌘' ? '↩' : 'Enter'}</Kbd>
          </KbdGroup>
          {/* The same verb as the button it presses. */}
          {action === 'trash'
            ? t('dedupe.action.trash')
            : t('dedupe.keys.apply')}
        </li>
      </ul>

      <p role="status" aria-live="polite" className="sr-only">
        {delta && delta.delta !== 0
          ? t('dedupe.deltaAnnounce', {
              delta: deltaText(delta),
              total: delta.total,
            })
          : ''}
      </p>
    </section>
  );
}
