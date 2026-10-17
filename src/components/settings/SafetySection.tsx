import { FolderOpen } from 'lucide-react';
import { useState } from 'react';

import { Alert } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import { errorKey, toBackendError, type BackendError } from '@/ipc/errors';
import { useSettingsStore } from '@/stores/settingsStore';

import {
  Field,
  NumberSetting,
  settingId,
  SwitchSetting,
  useFieldError,
} from './Field';

export function SafetySection() {
  const t = useT();
  const s = useSettingsStore((st) => st.settings!.safety);
  const update = useSettingsStore((st) => st.update);
  const dirError = useFieldError('safety.backup_dir');
  const [dir, setDir] = useState<string | null>(null);
  const [error, setError] = useState<BackendError | null>(null);
  const commitDir = async (value: string) => {
    if (
      (await update({ safety: { backup_dir: value.trim() || null } }))
        .length === 0
    )
      setDir(null);
  };
  const id = settingId('safety.backup_dir');
  return (
    <div className="settings-group">
      <SwitchSetting
        id={settingId('safety.backup')}
        name="backup"
        label={t('settings.safety.backup')}
        hint={t('settings.safety.backupHint')}
        checked
        disabled
      />
      <Field
        id={id}
        label={t('settings.safety.backupDir')}
        hint={t('settings.safety.backupDirHint')}
        error={dirError}
      >
        <Input
          name={id}
          mono
          className="max-w-md"
          value={dir ?? s.backup_dir ?? ''}
          onChange={(e) => setDir(e.target.value)}
          onBlur={() => dir !== null && void commitDir(dir)}
        />
      </Field>
      <Button
        className="self-start"
        onClick={async () => {
          setError(null);
          let picked: string | null;
          try {
            picked = await (await getBackend()).pickFolder();
          } catch (e) {
            setError(toBackendError(e));
            return;
          }
          if (picked) await commitDir(picked);
        }}
      >
        <FolderOpen aria-hidden />
        {t('settings.safety.choose')}
      </Button>
      {error && (
        <Alert variant="danger">
          {t(errorKey(error.code), { detail: error.detail ?? '' })}
        </Alert>
      )}
      <SwitchSetting
        id={settingId('safety.dry_run_default')}
        name="dry-run"
        label={t('settings.safety.dryRun')}
        checked={s.dry_run_default}
        onCheckedChange={(on) =>
          void update({ safety: { dry_run_default: on } })
        }
      />
      <SwitchSetting
        id={settingId('safety.clean_apple_double')}
        name="apple-double"
        label={t('settings.safety.appleDouble')}
        checked={s.clean_apple_double}
        onCheckedChange={(on) =>
          void update({ safety: { clean_apple_double: on } })
        }
      />
      <NumberSetting
        path="safety.undo_depth"
        label={t('settings.safety.undoDepth')}
        value={s.undo_depth}
        toPatch={(v) => ({ safety: { undo_depth: v } })}
      />
    </div>
  );
}
