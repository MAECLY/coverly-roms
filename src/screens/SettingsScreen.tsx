import { ArrowLeft } from 'lucide-react';
import {
  Fragment,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';

import { AboutSection } from '@/components/settings/AboutSection';
import { AdvancedSection } from '@/components/settings/AdvancedSection';
import { ArtSection } from '@/components/settings/ArtSection';
import { DuplicatesSection } from '@/components/settings/DuplicatesSection';
import { GeneralSection } from '@/components/settings/GeneralSection';
import { LibrarySection } from '@/components/settings/LibrarySection';
import { OrganizeSection } from '@/components/settings/OrganizeSection';
import { MatchingSection } from '@/components/settings/MatchingSection';
import { NetworkSection } from '@/components/settings/NetworkSection';
import { ProfilesSection } from '@/components/settings/ProfilesSection';
import type { SettingEntry } from '@/components/settings/registry';
import { SafetySection } from '@/components/settings/SafetySection';
import { searchSettings } from '@/components/settings/search';
import {
  SettingsResults,
  SettingsSearchField,
} from '@/components/settings/SettingsSearch';
import { SourcesSection } from '@/components/settings/SourcesSection';
import { Alert } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { TooltipProvider } from '@/components/ui/tooltip';
import { useMediaQuery } from '@/hooks/useMediaQuery';
import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import { errorKey } from '@/ipc/errors';
import { isMac, isTyping } from '@/lib/keyboard';
import { useLibraryStore } from '@/stores/libraryStore';
import {
  SETTINGS_GROUPS,
  SETTINGS_SECTIONS,
  useNavStore,
  type SettingsSection,
} from '@/stores/navStore';
import { cn } from '@/lib/utils';
import { useSettingsStore } from '@/stores/settingsStore';

/** Each section's panel. */
// eslint-disable-next-line react-refresh/only-export-components -- the section registry Tasks 13–14 extend (plan interface)
export const SECTION_COMPONENTS: Record<
  SettingsSection,
  () => React.JSX.Element
> = {
  general: GeneralSection,
  library: LibrarySection,
  organize: OrganizeSection,
  duplicates: DuplicatesSection,
  art: ArtSection,
  matching: MatchingSection,
  sources: SourcesSection,
  network: NetworkSection,
  safety: SafetySection,
  advanced: AdvancedSection,
  profiles: ProfilesSection,
  about: AboutSection,
};

/** The pending unmark of each marked row: a second arrival restarts it. */
const unmark = new WeakMap<HTMLElement, number>();

/** Scroll a setting into view, focus it and mark its row once (spec §6.1). */
function arrive(el: HTMLElement) {
  const row = el.closest<HTMLElement>('[data-setting-row]') ?? el;
  const reduce = window.matchMedia?.(
    '(prefers-reduced-motion: reduce)',
  ).matches;
  row.scrollIntoView?.({
    block: 'center',
    behavior: reduce ? 'auto' : 'smooth',
  });
  el.focus({ preventScroll: true });
  row.dataset.arrived = 'true';
  window.clearTimeout(unmark.get(row));
  unmark.set(
    row,
    window.setTimeout(() => {
      delete row.dataset.arrived;
      unmark.delete(row);
    }, 1200),
  );
}

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
  const focusSetting = useNavStore((s) => s.focusSetting);
  const openFolds = useNavStore((s) => s.openFolds);
  const heading = useRef<HTMLHeadingElement>(null);
  const locale = useContext(LocaleContext);
  const searchRef = useRef<HTMLInputElement>(null);
  const [query, setQueryState] = useState('');
  const [active, setActive] = useState(0);
  const results = useMemo(() => searchSettings(query, locale), [query, locale]);
  // The live count waits for typing to stop (spec §6.5); a cleared field
  // forgets it at once, so a new search never shows the last one's count.
  const [settled, setSettled] = useState('');
  useEffect(() => {
    if (query === '') return;
    const timer = window.setTimeout(() => setSettled(query), 300);
    return () => window.clearTimeout(timer);
  }, [query]);
  const settledResults = useMemo(
    () => (settled === query ? results : searchSettings(settled, locale)),
    [settled, query, results, locale],
  );
  function setQuery(q: string) {
    setQueryState(q);
    setActive(0);
    if (q === '') setSettled('');
  }
  function choose(entry: SettingEntry) {
    setQuery('');
    useNavStore.getState().openSettings(entry.section, { focus: entry.id });
  }
  function focusTab() {
    tabList.current
      ?.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]')
      ?.focus();
  }
  // ⌘F on macOS, Ctrl+F elsewhere, and `/` outside a text field focus the
  // search (D9). Bubbling, like the library screen, so a dialog or a
  // control that handled the key wins.
  useEffect(() => {
    function onKey(ev: KeyboardEvent) {
      if (ev.defaultPrevented || ev.isComposing || ev.altKey) return;
      if (
        document.querySelector(
          '[role="dialog"][data-state="open"],[role="alertdialog"][data-state="open"]',
        )
      )
        return;
      const f = ev.key.toLowerCase() === 'f' || ev.code === 'KeyF';
      const find =
        f && (isMac() ? ev.metaKey && !ev.ctrlKey : ev.ctrlKey && !ev.metaKey);
      const slash =
        ev.key === '/' && !ev.metaKey && !ev.ctrlKey && !isTyping(ev.target);
      if (!find && !slash) return;
      ev.preventDefault();
      searchRef.current?.focus();
      searchRef.current?.select();
    }
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);
  // Narrow windows show the tabs as one row: the arrow keys follow (D14).
  const narrow = useMediaQuery('(max-width: 40rem)');
  const tabList = useRef<HTMLDivElement>(null);
  useEffect(() => {
    tabList.current
      ?.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]')
      ?.scrollIntoView?.({ block: 'nearest', inline: 'nearest' });
  }, [section, narrow]);
  // The screen replaces the one that opened it: focus starts at its title,
  // unless it was opened to land on one setting.
  useEffect(() => {
    if (useNavStore.getState().focusSetting === null) heading.current?.focus();
  }, []);
  useEffect(() => {
    // Root loads them at start; this covers a screen shown without it.
    if (useSettingsStore.getState().status === 'idle')
      void useSettingsStore.getState().load();
  }, []);
  const libraryPath = useLibraryStore((s) =>
    s.status === 'ready' ? s.path : null,
  );
  const Panel = SECTION_COMPONENTS[section];
  // Land on the setting it was opened for once its panel is on screen.
  useEffect(() => {
    if (!focusSetting) return;
    if (status !== 'ready' && section !== 'about' && status !== 'error') return;
    const land = (): boolean => {
      const nav = useNavStore.getState();
      const el = document.getElementById(focusSetting);
      if (!el) return false;
      // Folded content is mounted but hidden (SP-3): open it, land next
      // render.
      const fold = el.closest<HTMLElement>('[data-advanced-fold]')?.dataset
        .advancedFold;
      if (fold && !nav.openFolds.includes(fold)) {
        nav.setFold(fold, true);
        return true;
      }
      arrive(el);
      nav.clearFocusSetting();
      return true;
    };
    if (land()) return;
    // A tab switch mounts its panel a render later (Radix Presence): look
    // once more before landing on the tab instead.
    const retry = window.setTimeout(() => {
      if (land()) return;
      useNavStore.getState().clearFocusSetting();
      document
        .querySelector<HTMLElement>('[role="tab"][aria-selected="true"]')
        ?.focus();
    }, 0);
    return () => window.clearTimeout(retry);
  }, [focusSetting, section, status, openFolds]);

  let body: React.JSX.Element;
  if (status === 'ready' || section === 'about') body = <Panel />;
  else if (status === 'error' && loadError)
    body = (
      <div className="settings-group">
        <Alert
          variant="danger"
          action={
            <Button
              size="sm"
              onClick={() => void useSettingsStore.getState().load()}
            >
              {t('settings.retry')}
            </Button>
          }
        >
          {t(errorKey(loadError.code), { detail: loadError.detail ?? '' })}
        </Alert>
      </div>
    );
  else
    body = (
      <p role="status" className="m-0 text-ui text-cr-text-secondary">
        {t('settings.loading')}
      </p>
    );

  async function reopenNow() {
    if (!libraryPath) return;
    await useLibraryStore.getState().open(libraryPath);
    dismissReopen();
  }

  return (
    <TooltipProvider>
      <main className="settings-screen">
        <header className="settings-header">
          <Button onClick={close}>
            <ArrowLeft aria-hidden />
            {t('settings.back')}
          </Button>
          <h1 ref={heading} tabIndex={-1}>
            {t('settings.title')}
          </h1>
        </header>
        {(saveError || notice || (reopen && libraryPath)) && (
          <div className="grid gap-2 px-6 pt-3">
            {saveError && (
              <Alert variant="danger">
                {t(errorKey(saveError.code), {
                  detail: saveError.detail ?? '',
                })}
              </Alert>
            )}
            {notice && (
              <Alert
                variant="warning"
                role="status"
                action={
                  <Button size="sm" variant="ghost" onClick={dismissNotice}>
                    {t('settings.dismiss')}
                  </Button>
                }
              >
                <p className="m-0 [overflow-wrap:anywhere]">
                  {t(`settings.notice.${notice}`, {
                    backup: backup ?? '—',
                    fields: fixed.join(', ') || '—',
                  })}
                </p>
              </Alert>
            )}
            {reopen && libraryPath && (
              <Alert
                variant="info"
                action={
                  <Button
                    size="sm"
                    variant="primary"
                    onClick={() => void reopenNow()}
                  >
                    {t('settings.reopenNow')}
                  </Button>
                }
              >
                <p className="m-0">{t('settings.reopen')}</p>
              </Alert>
            )}
          </div>
        )}
        <Tabs
          orientation={narrow ? 'horizontal' : 'vertical'}
          value={section}
          onValueChange={(v) => {
            setQuery('');
            setSection(v as SettingsSection);
          }}
          className="settings-body gap-0 max-[40rem]:!flex-col"
        >
          <nav className="settings-nav">
            <SettingsSearchField
              ref={searchRef}
              query={query}
              results={results}
              active={active}
              onQuery={setQuery}
              onActive={setActive}
              onChoose={choose}
              onLeave={focusTab}
            />
            <TabsList ref={tabList} aria-label={t('settings.sections')}>
              {SETTINGS_SECTIONS.map((s, i) => {
                const group = SETTINGS_GROUPS[s];
                // A label before the first tab of each group: not a stop
                // for the arrow keys and not a role="group" (ARIA allows
                // only tabs in a tablist); its first tab names it once.
                const before = SETTINGS_SECTIONS[i - 1];
                const prev = before ? SETTINGS_GROUPS[before] : null;
                const first = group !== null && prev !== group;
                const labelId = `settings-group-${group}`;
                return (
                  <Fragment key={s}>
                    {first && (
                      <span
                        aria-hidden
                        id={labelId}
                        className="settings-nav__group"
                      >
                        {t(`settings.group.${group}`)}
                      </span>
                    )}
                    <TabsTrigger
                      value={s}
                      // The selected tab's own click leaves the results
                      // for its panel (a change of tab clears them too).
                      onClick={() => setQuery('')}
                      aria-describedby={first ? labelId : undefined}
                      data-group-start={first ? '' : undefined}
                    >
                      {t(`settings.section.${s}`)}
                    </TabsTrigger>
                  </Fragment>
                );
              })}
            </TabsList>
          </nav>
          {SETTINGS_SECTIONS.map((s) => (
            <TabsContent
              key={s}
              value={s}
              className={cn(
                'min-w-0 flex-1 overflow-y-auto px-6 pb-8 pt-5',
                query !== '' && 'hidden',
              )}
            >
              {s === section && body}
            </TabsContent>
          ))}
          {query !== '' && (
            <SettingsResults
              results={results}
              active={active}
              settled={settled}
              settledCount={settledResults.length}
              onActive={setActive}
              onChoose={choose}
            />
          )}
        </Tabs>
      </main>
    </TooltipProvider>
  );
}
