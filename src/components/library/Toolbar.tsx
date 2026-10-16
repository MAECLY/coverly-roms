import type { RefObject } from 'react';

import { LanguageSwitch } from '@/components/LanguageSwitch';
import { useT } from '@/i18n/useT';
import type { LibraryView } from '@/ipc/types';
import { activeFilterCount } from '@/lib/filters';
import { useFilterStore } from '@/stores/filterStore';
import { isRunning, useJobStore } from '@/stores/jobStore';
import { useLibraryStore } from '@/stores/libraryStore';
import { useViewStore } from '@/stores/viewStore';

export function Toolbar({
  view,
  searchRef,
  onSettings,
  onFill,
  onJobs,
}: {
  view: LibraryView;
  searchRef: RefObject<HTMLInputElement | null>;
  onSettings(): void;
  onFill(): void;
  onJobs(): void;
}) {
  const t = useT();
  const filters = useFilterStore((s) => s.filters);
  const setQuery = useFilterStore((s) => s.setQuery);
  const layout = useViewStore((s) => s.layout);
  const setLayout = useViewStore((s) => s.setLayout);
  const filtersOpen = useViewStore((s) => s.filtersOpen);
  const toggleFilters = useViewStore((s) => s.toggleFilters);
  const close = useLibraryStore((s) => s.close);
  const job = useJobStore((s) => s.job);
  const active = activeFilterCount(filters);

  return (
    <header className="toolbar">
      <div className="toolbar-title">
        <h1 className="toolbar-name truncate" title={view.root}>
          {view.name}
        </h1>
        <span className="hint">{view.profile.name}</span>
      </div>
      <input
        ref={searchRef}
        type="search"
        name="search"
        className="text-input toolbar-search"
        aria-label={t('toolbar.search')}
        placeholder={t('toolbar.searchPlaceholder')}
        value={filters.query}
        onChange={(e) => setQuery(e.target.value)}
      />
      <button
        type="button"
        className="ghost-button"
        aria-pressed={filtersOpen}
        aria-controls="filter-panel"
        onClick={toggleFilters}
      >
        {active > 0
          ? t('toolbar.filtersActive', { n: active })
          : t('toolbar.filters')}
      </button>
      <div role="group" aria-label={t('toolbar.layout')} className="segmented">
        <button
          type="button"
          className="ghost-button"
          aria-pressed={layout === 'grid'}
          onClick={() => setLayout('grid')}
        >
          {t('toolbar.grid')}
        </button>
        <button
          type="button"
          className="ghost-button"
          aria-pressed={layout === 'list'}
          onClick={() => setLayout('list')}
        >
          {t('toolbar.list')}
        </button>
      </div>
      <span className="toolbar-spacer" />
      <button type="button" className="primary-button" onClick={onFill}>
        {t('toolbar.fill')}
      </button>
      <button type="button" className="ghost-button" onClick={onJobs}>
        {isRunning(job)
          ? t('toolbar.jobsRunning')
          : job?.state === 'paused'
            ? t('toolbar.jobsPaused')
            : t('toolbar.jobs')}
      </button>
      <button type="button" className="ghost-button" onClick={onSettings}>
        {t('start.settings')}
      </button>
      <LanguageSwitch />
      <button type="button" className="ghost-button" onClick={close}>
        {t('library.close')}
      </button>
    </header>
  );
}
