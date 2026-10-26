import type { ReactNode } from 'react';

import { Field, FieldControl, FieldLabel } from '@/components/ui/field';
import { NativeSelect } from '@/components/ui/select';
import { useT } from '@/i18n/useT';
import {
  ORGANIZE_DEDUPE_MODES,
  type NameMode,
  type OrganizeDedupeMode,
  type OrganizePlan,
  type OrganizeRequest,
  type ProfileInfo,
  type TransferMode,
} from '@/ipc/types';

/** One labelled picker of the bar. */
function Picker({ label, children }: { label: string; children: ReactNode }) {
  return (
    <Field className="w-auto min-w-[10rem] max-w-[18rem]">
      <FieldLabel className="text-caption text-cr-text-secondary">
        {label}
      </FieldLabel>
      <FieldControl>{children}</FieldControl>
    </Field>
  );
}

const HERE = 'here';
const OTHER = 'other';

/** The i18n key of each duplicates step. */
const DEDUPE_LABEL: Record<OrganizeDedupeMode, string> = {
  'set-aside': 'organize.dedupe.quarantine',
  report: 'organize.dedupe.report',
  off: 'organize.dedupe.off',
};

interface Props {
  plan: OrganizePlan;
  /** The duplicates step the plan was made with. */
  duplicates: OrganizeDedupeMode;
  profiles: ProfileInfo[];
  disabled?: boolean;
  /** Layout, target and folders are fixed (a library's loose files). */
  locked?: boolean;
  onChange(patch: Partial<OrganizeRequest>): void;
  /** "Another folder…": the screen asks for one and re-plans with it. */
  onPickTarget(): void;
}

/**
 * Layout, target, folders, mode and names of the proposal (spec §4.2.1).
 * Every value shown is the one the plan on screen was made with.
 */
export function OptionsBar({
  plan,
  duplicates,
  profiles,
  disabled = false,
  locked = false,
  onChange,
  onPickTarget,
}: Props) {
  const t = useT();
  const layout = profiles.find((p) => p.profile.id === plan.layout);
  const romsRoot = layout?.profile.roms_root ?? '';
  const nested = plan.roms_dir !== plan.target_root;
  const elsewhere = plan.target_root !== plan.source_root;
  const moveDisabled = t('organize.mode.moveDisabled');
  return (
    <div className="flex flex-wrap items-end gap-3 rounded-lg border border-cr-border bg-cr-surface p-3">
      <Picker label={t('organize.layout')}>
        <NativeSelect
          size="sm"
          value={plan.layout}
          disabled={disabled || locked}
          onChange={(e) => onChange({ layout: e.target.value })}
        >
          {!layout && <option value={plan.layout}>{plan.layout}</option>}
          {profiles.map((p) => (
            <option key={p.profile.id} value={p.profile.id}>
              {p.profile.name}
            </option>
          ))}
        </NativeSelect>
      </Picker>
      <Picker label={t('organize.target.label')}>
        <NativeSelect
          size="sm"
          value={elsewhere ? plan.target_root : HERE}
          disabled={disabled || locked}
          onChange={(e) => {
            const v = e.target.value;
            if (v === OTHER) onPickTarget();
            else if (v === HERE) onChange({ target: null });
          }}
        >
          <option value={HERE}>{t('organize.target.here')}</option>
          {elsewhere && (
            <option value={plan.target_root}>{plan.target_root}</option>
          )}
          <option value={OTHER}>{t('organize.target.other')}</option>
        </NativeSelect>
      </Picker>
      {romsRoot !== '' && (
        <Picker label={t('organize.nest.label')}>
          <NativeSelect
            size="sm"
            value={nested ? 'roms' : 'flat'}
            disabled={disabled || locked}
            onChange={(e) => onChange({ flat: e.target.value === 'flat' })}
          >
            <option value="roms">
              {t('organize.nest.roms', { folder: romsRoot })}
            </option>
            <option value="flat">{t('organize.nest.flat')}</option>
          </NativeSelect>
        </Picker>
      )}
      <Picker label={t('organize.mode.label')}>
        <NativeSelect
          size="sm"
          value={plan.mode}
          disabled={disabled}
          onChange={(e) => onChange({ mode: e.target.value as TransferMode })}
        >
          <option
            value="move"
            disabled={!plan.move_allowed}
            title={plan.move_allowed ? undefined : moveDisabled}
          >
            {t('organize.mode.move')}
          </option>
          <option value="copy">{t('organize.mode.copy')}</option>
        </NativeSelect>
      </Picker>
      <Picker label={t('organize.names.label')}>
        <NativeSelect
          size="sm"
          value={plan.names}
          disabled={disabled}
          onChange={(e) => onChange({ names: e.target.value as NameMode })}
        >
          <option value="keep">{t('organize.names.keep')}</option>
          <option value="catalogue">{t('organize.names.catalogue')}</option>
        </NativeSelect>
      </Picker>
      <Picker label={t('organize.dedupe.picker')}>
        <NativeSelect
          size="sm"
          value={duplicates}
          disabled={disabled}
          onChange={(e) =>
            onChange({ duplicates: e.target.value as OrganizeDedupeMode })
          }
        >
          {ORGANIZE_DEDUPE_MODES.map((m) => (
            <option key={m} value={m}>
              {t(DEDUPE_LABEL[m])}
            </option>
          ))}
        </NativeSelect>
      </Picker>
    </div>
  );
}
