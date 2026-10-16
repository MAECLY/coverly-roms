import { useEffect, useRef, type KeyboardEvent } from 'react';

import { AboutSection } from '@/components/settings/AboutSection';
import { AdvancedSection } from '@/components/settings/AdvancedSection';
import { ArtSection } from '@/components/settings/ArtSection';
import { GeneralSection } from '@/components/settings/GeneralSection';
import { LibrarySection } from '@/components/settings/LibrarySection';
import { MatchingSection } from '@/components/settings/MatchingSection';
import { NetworkSection } from '@/components/settings/NetworkSection';
import { SafetySection } from '@/components/settings/SafetySection';
import { SourcesSection } from '@/components/settings/SourcesSection';
import { useT } from '@/i18n/useT';
import { errorKey } from '@/ipc/errors';
import { useLibraryStore } from '@/stores/libraryStore';
import {
  SETTINGS_SECTIONS,
  useNavStore,
  type SettingsSection,
} from '@/stores/navStore';
import { useSettingsStore } from '@/stores/settingsStore';

function Pending() {
  const t = useT();
  return <p className="hint">{t('settings.loading')}</p>;
}

/** Each section's panel (Tasks 13–14 replace the pending ones). */
// eslint-disable-next-line react-refresh/only-export-components -- the section registry Tasks 13–14 extend (plan interface)
export const SECTION_COMPONENTS: Record<
  SettingsSection,
  () => React.JSX.Element
> = {
  general: GeneralSection,
  library: LibrarySection,
  art: ArtSection,
  matching: MatchingSection,
  sources: SourcesSection,
  network: NetworkSection,
  safety: SafetySection,
  advanced: AdvancedSection,
  profiles: Pending,
  about: AboutSection,
};

export function SettingsScreen() {
  const t = useT();
  const section = useNavStore((s) => s.section);
  const setSection = useNavStore((s) => s.setSection);
  const close = useNavStore((s) => s.closeSettings);
  const status = useSettingsStore((s) => s.status);
  const notice = useSettingsStore((s) => s.notice);
  const backup = useSettingsStore((s) => s.backup);
  const fixed = useSettingsStore((s) => s.fixed);
  const reopen = useSettingsStore((s) => s.reopen);
  const dismissNotice = useSettingsStore((s) => s.dismissNotice);
  const dismissReopen = useSettingsStore((s) => s.dismissReopen);
  const loadError = useSettingsStore((s) => s.error);
  const saveError = useSettingsStore((s) => s.saveError);
  const tabRefs = useRef(new Map<SettingsSection, HTMLButtonElement>());
  const heading = useRef<HTMLHeadingElement>(null);
  // The screen replaces the one that opened it: focus starts at its title.
  useEffect(() => heading.current?.focus(), []);
  useEffect(() => {
    // Root loads them at start; this covers a screen shown without it.
    if (useSettingsStore.getState().status === 'idle')
      void useSettingsStore.getState().load();
  }, []);
  const libraryPath = useLibraryStore((s) =>
    s.status === 'ready' ? s.path : null,
  );
  const Panel = SECTION_COMPONENTS[section];

  /** WAI-ARIA vertical tabs: Up/Down (and Home/End) move and select. */
  function onTabKey(ev: KeyboardEvent<HTMLButtonElement>, id: SettingsSection) {
    const n = SETTINGS_SECTIONS.length;
    const at = SETTINGS_SECTIONS.indexOf(id);
    let next: number;
    if (ev.key === 'ArrowDown') next = (at + 1) % n;
    else if (ev.key === 'ArrowUp') next = (at - 1 + n) % n;
    else if (ev.key === 'Home') next = 0;
    else if (ev.key === 'End') next = n - 1;
    else return;
    ev.preventDefault();
    const target = SETTINGS_SECTIONS[next]!;
    setSection(target);
    tabRefs.current.get(target)?.focus();
  }

  let body: React.JSX.Element;
  if (status === 'ready' || section === 'about') body = <Panel />;
  else if (status === 'error' && loadError)
    body = (
      <div className="settings-group">
        <p role="alert" className="error-banner">
          {t(errorKey(loadError.code), { detail: loadError.detail ?? '' })}
        </p>
        <button
          type="button"
          className="ghost-button"
          onClick={() => void useSettingsStore.getState().load()}
        >
          {t('settings.retry')}
        </button>
      </div>
    );
  else body = <p role="status">{t('settings.loading')}</p>;

  async function reopenNow() {
    if (!libraryPath) return;
    await useLibraryStore.getState().open(libraryPath);
    dismissReopen();
  }

  return (
    <main className="settings-screen">
      <header className="settings-header">
        <button type="button" className="ghost-button" onClick={close}>
          {t('settings.back')}
        </button>
        <h1 ref={heading} tabIndex={-1}>
          {t('settings.title')}
        </h1>
      </header>
      {saveError && (
        <p role="alert" className="error-banner">
          {t(errorKey(saveError.code), { detail: saveError.detail ?? '' })}
        </p>
      )}
      {notice && (
        <div role="status" className="notice-banner">
          <p>
            {t(`settings.notice.${notice}`, {
              backup: backup ?? '—',
              fields: fixed.join(', ') || '—',
            })}
          </p>
          <button
            type="button"
            className="ghost-button"
            onClick={dismissNotice}
          >
            {t('settings.dismiss')}
          </button>
        </div>
      )}
      {reopen && libraryPath && (
        <div role="status" className="notice-banner">
          <p>{t('settings.reopen')}</p>
          <button
            type="button"
            className="primary-button"
            onClick={() => void reopenNow()}
          >
            {t('settings.reopenNow')}
          </button>
        </div>
      )}
      <div className="settings-body">
        <nav className="settings-nav">
          <div
            role="tablist"
            aria-label={t('settings.sections')}
            aria-orientation="vertical"
          >
            {SETTINGS_SECTIONS.map((s) => (
              <button
                key={s}
                type="button"
                role="tab"
                id={`settings-tab-${s}`}
                aria-selected={s === section}
                aria-controls={`settings-panel-${s}`}
                tabIndex={s === section ? 0 : -1}
                ref={(el) => {
                  if (el) tabRefs.current.set(s, el);
                  else tabRefs.current.delete(s);
                }}
                onKeyDown={(e) => onTabKey(e, s)}
                className="settings-tab"
                onClick={() => setSection(s)}
              >
                {t(`settings.section.${s}`)}
              </button>
            ))}
          </div>
        </nav>
        <section
          role="tabpanel"
          id={`settings-panel-${section}`}
          aria-labelledby={`settings-tab-${section}`}
          className="settings-panel"
        >
          {body}
        </section>
      </div>
    </main>
  );
}
