import { Copy, Pencil, Plus, Trash2 } from 'lucide-react';
import { useEffect, useState } from 'react';

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
} from '@/components/ui/alert-dialog';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import { errorKey, toBackendError, type BackendError } from '@/ipc/errors';
import type { ProfileFile, ProfileInfo } from '@/ipc/types';
import { profileFromBuiltin } from '@/lib/profileTemplate';
import { useSettingsStore } from '@/stores/settingsStore';

import { hintText } from './Field';
import { BLANK_PROFILE, ProfileEditor } from './ProfileEditor';

export function ProfilesSection() {
  const t = useT();
  const [profiles, setProfiles] = useState<ProfileInfo[]>([]);
  const [editing, setEditing] = useState<{
    initial: ProfileFile;
    replaces: string | null;
  } | null>(null);
  /** The profile the confirmation is about; kept while it closes. */
  const [deleting, setDeleting] = useState<ProfileInfo | null>(null);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [error, setError] = useState<BackendError | null>(null);
  useEffect(() => {
    let alive = true;
    void getBackend()
      .then((b) => b.listProfiles())
      .then(
        (p) => alive && setProfiles(p),
        (e) => alive && setError(toBackendError(e)),
      );
    return () => {
      alive = false;
    };
  }, []);

  async function remove(p: ProfileInfo) {
    setConfirmOpen(false);
    try {
      setProfiles(await (await getBackend()).deleteProfile(p.profile.id));
      // The default profile / max size of a deleted profile changed in the backend.
      await useSettingsStore.getState().refresh();
    } catch (e) {
      setError(toBackendError(e));
    }
  }

  if (editing) {
    return (
      <ProfileEditor
        initial={editing.initial}
        replaces={editing.replaces}
        onSaved={(list) => {
          setProfiles(list);
          setEditing(null);
          // A rename moves the default profile and its max size in the backend.
          void useSettingsStore.getState().refresh();
        }}
        onCancel={() => setEditing(null)}
      />
    );
  }
  return (
    <div className="settings-group">
      <p className={hintText}>{t('profiles.hint')}</p>
      <Button
        variant="primary"
        className="self-start"
        onClick={() => setEditing({ initial: BLANK_PROFILE, replaces: null })}
      >
        <Plus aria-hidden />
        {t('profiles.new')}
      </Button>
      <ul
        aria-label={t('profiles.list')}
        className="m-0 grid list-none divide-y divide-cr-border rounded-md border border-cr-border p-0"
      >
        {profiles.map((p) => (
          <li
            key={p.profile.id}
            aria-label={p.profile.name}
            className="flex flex-wrap items-center gap-2 px-3 py-2"
          >
            <span
              className="min-w-32 flex-1 truncate text-ui font-medium"
              title={p.profile.id}
            >
              {p.profile.name}
            </span>
            {p.builtin ? (
              <>
                <Badge>{t('profiles.builtin')}</Badge>
                <Button
                  size="sm"
                  aria-label={t('profiles.duplicate', { name: p.profile.name })}
                  onClick={() =>
                    setEditing({
                      initial: profileFromBuiltin(
                        p,
                        `${p.profile.id}-copy`,
                        t('profiles.copyName', { name: p.profile.name }),
                      ),
                      replaces: null,
                    })
                  }
                >
                  <Copy aria-hidden />
                  {t('profiles.duplicateShort')}
                </Button>
              </>
            ) : (
              <>
                <Button
                  size="sm"
                  aria-label={t('profiles.edit', { name: p.profile.name })}
                  onClick={() =>
                    setEditing({ initial: p.profile, replaces: p.profile.id })
                  }
                >
                  <Pencil aria-hidden />
                  {t('profiles.editShort')}
                </Button>
                <Button
                  size="sm"
                  className="text-cr-danger hover:border-cr-danger"
                  aria-label={t('profiles.delete', { name: p.profile.name })}
                  onClick={() => {
                    setDeleting(p);
                    setConfirmOpen(true);
                  }}
                >
                  <Trash2 aria-hidden />
                  {t('profiles.deleteShort')}
                </Button>
              </>
            )}
          </li>
        ))}
      </ul>
      {error && (
        <Alert variant="danger">
          {t(errorKey(error.code), { detail: error.detail ?? '' })}
        </Alert>
      )}
      <AlertDialog open={confirmOpen} onOpenChange={setConfirmOpen}>
        {deleting && (
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>
                {t('profiles.deleteTitle', { name: deleting.profile.name })}
              </AlertDialogTitle>
              <AlertDialogDescription>
                {t('profiles.deleteBody')}
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel>{t('profiles.cancel')}</AlertDialogCancel>
              <AlertDialogAction
                variant="destructive"
                onClick={() => void remove(deleting)}
              >
                {t('profiles.deleteYes')}
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        )}
      </AlertDialog>
    </div>
  );
}
