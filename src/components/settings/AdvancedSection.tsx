import { FolderOpen } from 'lucide-react';
import { useState } from 'react';

import { Alert } from '@/components/ui/alert';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from '@/components/ui/alert-dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { NativeSelect } from '@/components/ui/select';
import { useT } from '@/i18n/useT';
import { getBackend, type Backend } from '@/ipc/backend';
import { errorKey, toBackendError, type BackendError } from '@/ipc/errors';
import type { LogLevel } from '@/ipc/types';
import { useSettingsStore } from '@/stores/settingsStore';

import { Field, settingId, useFieldError } from './Field';

export function AdvancedSection() {
  const t = useT();
  const a = useSettingsStore((s) => s.settings!.advanced);
  const update = useSettingsStore((s) => s.update);
  const reset = useSettingsStore((s) => s.reset);
  const dirError = useFieldError('advanced.custom_dir');
  const [dir, setDir] = useState<string | null>(null);
  const [confirm, setConfirm] = useState(false);
  const [error, setError] = useState<BackendError | null>(null);
  async function openFolder(op: (b: Backend) => Promise<void>) {
    setError(null);
    try {
      await op(await getBackend());
    } catch (e) {
      setError(toBackendError(e));
    }
  }
  const id = settingId('advanced.custom_dir');
  return (
    <div className="settings-group">
      <Field
        id={settingId('advanced.log_level')}
        label={t('settings.advanced.logLevel')}
      >
        <NativeSelect
          name="log-level"
          className="max-w-xs"
          value={a.log_level}
          onChange={(e) =>
            void update({ advanced: { log_level: e.target.value as LogLevel } })
          }
        >
          {(['error', 'warn', 'info', 'debug'] as const).map((l) => (
            <option key={l} value={l}>
              {t(`settings.advanced.levels.${l}`)}
            </option>
          ))}
        </NativeSelect>
      </Field>
      <div className="flex flex-wrap gap-2">
        <Button onClick={() => void openFolder((b) => b.openLogsFolder())}>
          <FolderOpen aria-hidden />
          {t('settings.advanced.openLogs')}
        </Button>
        <Button onClick={() => void openFolder((b) => b.openConfigFolder())}>
          <FolderOpen aria-hidden />
          {t('settings.advanced.openConfig')}
        </Button>
      </div>
      {error && (
        <Alert variant="danger">
          {t(errorKey(error.code), { detail: error.detail ?? '' })}
        </Alert>
      )}
      <Field
        id={id}
        label={t('settings.advanced.customDir')}
        hint={t('settings.advanced.customDirHint')}
        error={dirError}
      >
        <Input
          name={id}
          mono
          className="max-w-md"
          value={dir ?? a.custom_dir ?? ''}
          onChange={(e) => setDir(e.target.value)}
          onBlur={async () => {
            if (dir === null) return;
            if (
              (await update({ advanced: { custom_dir: dir.trim() || null } }))
                .length === 0
            )
              setDir(null);
          }}
        />
      </Field>
      <AlertDialog open={confirm} onOpenChange={setConfirm}>
        <AlertDialogTrigger asChild>
          <Button className="self-start text-cr-danger hover:border-cr-danger">
            {t('settings.advanced.reset')}
          </Button>
        </AlertDialogTrigger>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {t('settings.advanced.resetTitle')}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {t('settings.advanced.resetBody')}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>
              {t('settings.advanced.cancel')}
            </AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              onClick={() => void reset()}
            >
              {t('settings.advanced.resetYes')}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
