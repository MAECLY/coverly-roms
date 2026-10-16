import { useEffect, useRef, useState } from 'react';

import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import { errorKey, toBackendError, type BackendError } from '@/ipc/errors';
import { useLibraryStore } from '@/stores/libraryStore';

const FOCUSABLE =
  'button:not([disabled]), textarea:not([disabled]), input:not([disabled]), select:not([disabled]), [href], [tabindex]:not([tabindex="-1"])';

/** Keeps Tab and Shift+Tab cycling inside `root`. */
function trapTab(ev: KeyboardEvent, root: HTMLElement | null) {
  if (!root) return;
  const items = Array.from(root.querySelectorAll<HTMLElement>(FOCUSABLE));
  const first = items[0];
  const last = items[items.length - 1];
  const active = document.activeElement;
  if (!first || !last) {
    ev.preventDefault();
    root.focus();
  } else if (ev.shiftKey && (active === first || active === root)) {
    ev.preventDefault();
    last.focus();
  } else if (!ev.shiftKey && (active === last || !root.contains(active))) {
    ev.preventDefault();
    first.focus();
  }
}

/** Edits the ignored folders; saving rescans the open library. */
export function LibrarySettingsDialog({ onClose }: { onClose(): void }) {
  const t = useT();
  const [text, setText] = useState<string | null>(null);
  const [error, setError] = useState<BackendError | null>(null);
  const dialogRef = useRef<HTMLDivElement>(null);

  // Modal focus: move focus in, keep Tab inside, give it back on close.
  useEffect(() => {
    const opener =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;
    dialogRef.current?.focus();
    return () => opener?.focus();
  }, []);

  useEffect(() => {
    let alive = true;
    void getBackend()
      .then((b) => b.librarySettings())
      .then(
        (s) => alive && setText(s.ignored_dirs.join('\n')),
        (e: unknown) => alive && setError(toBackendError(e)),
      );
    const onKey = (ev: KeyboardEvent) => {
      if (ev.key === 'Escape') onClose();
      else if (ev.key === 'Tab') trapTab(ev, dialogRef.current);
    };
    window.addEventListener('keydown', onKey);
    return () => {
      alive = false;
      window.removeEventListener('keydown', onKey);
    };
  }, [onClose]);

  async function save() {
    try {
      const backend = await getBackend();
      await backend.setLibrarySettings({
        ignored_dirs: (text ?? '').split('\n'),
      });
      onClose();
      const { status, path, open } = useLibraryStore.getState();
      if (status === 'ready' && path) await open(path);
    } catch (e) {
      setError(toBackendError(e));
    }
  }

  return (
    <div className="modal-backdrop">
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="library-settings-title"
        className="modal"
        ref={dialogRef}
        tabIndex={-1}
      >
        <h2 id="library-settings-title" className="section-title">
          {t('settings.title')}
        </h2>
        <label htmlFor="ignored-dirs" className="field-label">
          {t('settings.ignoredDirs')}
        </label>
        <textarea
          id="ignored-dirs"
          className="textarea"
          rows={6}
          value={text ?? ''}
          disabled={text === null}
          aria-describedby="ignored-dirs-hint"
          onChange={(e) => setText(e.target.value)}
        />
        <p id="ignored-dirs-hint" className="hint">
          {t('settings.ignoredDirsHint')}
        </p>
        {error && (
          <p role="alert" className="error-banner">
            {t(errorKey(error.code), { detail: error.detail ?? '' })}
          </p>
        )}
        <div className="modal-actions">
          <button type="button" className="ghost-button" onClick={onClose}>
            {t('settings.cancel')}
          </button>
          <button
            type="button"
            className="primary-button"
            disabled={text === null}
            onClick={() => void save()}
          >
            {t('settings.save')}
          </button>
        </div>
      </div>
    </div>
  );
}
