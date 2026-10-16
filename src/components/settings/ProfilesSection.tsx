import { useEffect, useState } from 'react';

import { Modal } from '@/components/Modal';
import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import { errorKey, toBackendError, type BackendError } from '@/ipc/errors';
import type { ProfileFile, ProfileInfo } from '@/ipc/types';
import { profileFromBuiltin } from '@/lib/profileTemplate';
import { useSettingsStore } from '@/stores/settingsStore';

import { BLANK_PROFILE, ProfileEditor } from './ProfileEditor';

export function ProfilesSection() {
  const t = useT();
  const [profiles, setProfiles] = useState<ProfileInfo[]>([]);
  const [editing, setEditing] = useState<{
    initial: ProfileFile;
    replaces: string | null;
  } | null>(null);
  const [deleting, setDeleting] = useState<ProfileInfo | null>(null);
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
    setDeleting(null);
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
      <p className="hint">{t('profiles.hint')}</p>
      <button
        type="button"
        className="primary-button"
        onClick={() => setEditing({ initial: BLANK_PROFILE, replaces: null })}
      >
        {t('profiles.new')}
      </button>
      <ul aria-label={t('profiles.list')} className="profile-list">
        {profiles.map((p) => (
          <li key={p.profile.id} aria-label={p.profile.name}>
            <span className="truncate" title={p.profile.id}>
              {p.profile.name}
            </span>
            {p.builtin ? (
              <>
                <span className="hint">{t('profiles.builtin')}</span>
                <button
                  type="button"
                  className="ghost-button"
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
                  {t('profiles.duplicateShort')}
                </button>
              </>
            ) : (
              <>
                <button
                  type="button"
                  className="ghost-button"
                  aria-label={t('profiles.edit', { name: p.profile.name })}
                  onClick={() =>
                    setEditing({ initial: p.profile, replaces: p.profile.id })
                  }
                >
                  {t('profiles.editShort')}
                </button>
                <button
                  type="button"
                  className="ghost-button"
                  aria-label={t('profiles.delete', { name: p.profile.name })}
                  onClick={() => setDeleting(p)}
                >
                  {t('profiles.deleteShort')}
                </button>
              </>
            )}
          </li>
        ))}
      </ul>
      {error && (
        <p role="alert" className="error-banner">
          {t(errorKey(error.code), { detail: error.detail ?? '' })}
        </p>
      )}
      {deleting && (
        <Modal
          labelledBy="delete-profile-title"
          onClose={() => setDeleting(null)}
        >
          <h2 id="delete-profile-title">
            {t('profiles.deleteTitle', { name: deleting.profile.name })}
          </h2>
          <p>{t('profiles.deleteBody')}</p>
          <div className="modal-actions">
            <button
              type="button"
              className="ghost-button"
              onClick={() => setDeleting(null)}
            >
              {t('profiles.cancel')}
            </button>
            <button
              type="button"
              className="primary-button"
              onClick={() => void remove(deleting)}
            >
              {t('profiles.deleteYes')}
            </button>
          </div>
        </Modal>
      )}
    </div>
  );
}
