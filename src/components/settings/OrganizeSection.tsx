import { NativeSelect } from '@/components/ui/select';
import { useT } from '@/i18n/useT';
import {
  ORGANIZE_DEDUPE_MODES,
  type FoundTool,
  type OrganizeDedupeMode,
  type OrganizeModeSetting,
  type OrganizeNameMode,
} from '@/ipc/types';
import { toolName } from '@/lib/organize';
import { useSettingsStore } from '@/stores/settingsStore';

import {
  AdvancedFold,
  Field,
  hintText,
  MonoIn,
  settingId,
  SwitchSetting,
  useFieldError,
} from './Field';
import { foldCount } from './registry';
import { useProfiles } from './useProfiles';

/** The i18n key of each duplicates step of Organise. */
const STEP_LABEL: Record<OrganizeDedupeMode, string> = {
  'set-aside': 'organize.dedupe.quarantine',
  report: 'organize.dedupe.report',
  off: 'organize.dedupe.off',
};

/** What was found, each tool as a mono readout (brand §5) inside the
 * translated line; or how to get one. */
function ExternalToolsHint({ found }: { found: FoundTool[] }) {
  const t = useT();
  if (found.length === 0)
    return <>{t('settings.organize.externalToolsNone')}</>;
  const list = found
    .map((f) => `${toolName(f.tool)} ${f.version} (${f.path})`)
    .join(', ');
  // Paths are long unbroken tokens: they wrap anywhere instead of pushing
  // the panel wider.
  return (
    <MonoIn
      text={t('settings.organize.externalToolsFound', { tools: '\u0000' })}
      value={list}
      className="[overflow-wrap:anywhere]"
    />
  );
}

/**
 * Settings › Organizar (spec §5.2): the defaults of the organise screen,
 * the duplicates step of a run (D15) visible; the external RAR programs
 * folded. Thresholds and lists stay in the settings file.
 */
export function OrganizeSection() {
  const t = useT();
  const org = useSettingsStore((s) => s.settings!.organize);
  const step = useSettingsStore((s) => s.settings!.dedupe.organize_step);
  const update = useSettingsStore((s) => s.update);
  const found = useSettingsStore((s) => s.externalToolsFound);
  const profiles = useProfiles();
  // Until the list loads, or for a layout it no longer has, the stored id
  // still shows.
  const listed = profiles.some((p) => p.profile.id === org.default_layout);
  return (
    <div className="settings-group">
      <Field
        id={settingId('organize.mode')}
        label={t('settings.organize.mode')}
        error={useFieldError('organize.mode')}
      >
        <NativeSelect
          name="organize-mode"
          className="max-w-xs"
          value={org.mode}
          onChange={(e) =>
            void update({
              organize: { mode: e.target.value as OrganizeModeSetting },
            })
          }
        >
          <option value="auto">{t('settings.organize.modeAuto')}</option>
          <option value="move">{t('settings.organize.modeMove')}</option>
          <option value="copy">{t('settings.organize.modeCopy')}</option>
        </NativeSelect>
      </Field>
      <Field
        id={settingId('organize.default_layout')}
        label={t('settings.organize.defaultLayout')}
        error={useFieldError('organize.default_layout')}
      >
        <NativeSelect
          name="organize-default-layout"
          className="max-w-xs"
          value={org.default_layout}
          onChange={(e) =>
            void update({ organize: { default_layout: e.target.value } })
          }
        >
          {!listed && (
            <option value={org.default_layout}>{org.default_layout}</option>
          )}
          {profiles.map((p) => (
            <option key={p.profile.id} value={p.profile.id}>
              {p.profile.name}
            </option>
          ))}
        </NativeSelect>
      </Field>
      <Field
        id={settingId('organize.names')}
        label={t('settings.organize.names')}
        error={useFieldError('organize.names')}
      >
        <NativeSelect
          name="organize-names"
          className="max-w-xs"
          value={org.names}
          onChange={(e) =>
            void update({
              organize: { names: e.target.value as OrganizeNameMode },
            })
          }
        >
          <option value="keep">{t('settings.organize.namesKeep')}</option>
          <option value="catalogue">
            {t('settings.organize.namesCatalogue')}
          </option>
        </NativeSelect>
      </Field>
      <SwitchSetting
        id={settingId('organize.nest_in_roms_root')}
        name="organize-nest-in-roms-root"
        label={t('settings.organize.nestInRoms')}
        checked={org.nest_in_roms_root}
        onCheckedChange={(on) =>
          void update({ organize: { nest_in_roms_root: on } })
        }
      />
      <SwitchSetting
        id={settingId('organize.download_dats')}
        name="organize-download-dats"
        label={t('settings.organize.downloadDats')}
        checked={org.download_dats}
        onCheckedChange={(on) =>
          void update({ organize: { download_dats: on } })
        }
      />
      <Field
        id={settingId('dedupe.organize_step')}
        label={t('settings.dedupe.organizeStep')}
      >
        <NativeSelect
          name="dedupe-organize-step"
          className="max-w-xs"
          value={step}
          onChange={(e) =>
            void update({
              dedupe: { organize_step: e.target.value as OrganizeDedupeMode },
            })
          }
        >
          {ORGANIZE_DEDUPE_MODES.map((m) => (
            <option key={m} value={m}>
              {t(STEP_LABEL[m])}
            </option>
          ))}
        </NativeSelect>
      </Field>
      <AdvancedFold
        id="organize-advanced"
        count={foldCount('organize-advanced')}
        errorPaths={['organize.external_tools']}
      >
        <SwitchSetting
          id={settingId('organize.external_tools')}
          name="organize-external-tools"
          label={t('settings.organize.externalTools')}
          hint={<ExternalToolsHint found={found} />}
          checked={org.external_tools === 'auto'}
          onCheckedChange={(on) =>
            void update({ organize: { external_tools: on ? 'auto' : 'off' } })
          }
        />
        <p className={hintText}>{t('settings.organize.advancedHint')}</p>
      </AdvancedFold>
    </div>
  );
}
