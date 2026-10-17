import { useT } from '@/i18n/useT';
import type {
  NameMode,
  OrganizePlan,
  OrganizeRequest,
  ProfileInfo,
  TransferMode,
} from '@/ipc/types';

const HERE = 'here';
const OTHER = 'other';

interface Props {
  plan: OrganizePlan;
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
    <div className="flex flex-wrap items-end gap-3">
      <label>
        <span className="field-label">{t('organize.layout')}</span>
        <select
          className="select"
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
        </select>
      </label>
      <label>
        <span className="field-label">{t('organize.target.label')}</span>
        <select
          className="select"
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
        </select>
      </label>
      {romsRoot !== '' && (
        <label>
          <span className="field-label">{t('organize.nest.label')}</span>
          <select
            className="select"
            value={nested ? 'roms' : 'flat'}
            disabled={disabled || locked}
            onChange={(e) => onChange({ flat: e.target.value === 'flat' })}
          >
            <option value="roms">
              {t('organize.nest.roms', { folder: romsRoot })}
            </option>
            <option value="flat">{t('organize.nest.flat')}</option>
          </select>
        </label>
      )}
      <label>
        <span className="field-label">{t('organize.mode.label')}</span>
        <select
          className="select"
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
        </select>
      </label>
      <label>
        <span className="field-label">{t('organize.names.label')}</span>
        <select
          className="select"
          value={plan.names}
          disabled={disabled}
          onChange={(e) => onChange({ names: e.target.value as NameMode })}
        >
          <option value="keep">{t('organize.names.keep')}</option>
          <option value="catalogue">{t('organize.names.catalogue')}</option>
        </select>
      </label>
    </div>
  );
}
