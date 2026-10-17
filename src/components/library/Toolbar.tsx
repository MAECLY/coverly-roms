import {
  ChevronLeft,
  FileText,
  LayoutGrid,
  List,
  ListChecks,
  Search,
  Settings,
  SlidersHorizontal,
} from 'lucide-react';
import type { RefObject } from 'react';

import { LanguageSwitch } from '@/components/LanguageSwitch';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Kbd } from '@/components/ui/kbd';
import { Spinner } from '@/components/ui/spinner';
import {
  SegmentedControl,
  SegmentedControlItem,
} from '@/components/ui/toggle-group';
import { useT } from '@/i18n/useT';
import type { LibraryView } from '@/ipc/types';
import { activeFilterCount } from '@/lib/filters';
import { useFilterStore } from '@/stores/filterStore';
import { isRunning, useJobStore } from '@/stores/jobStore';
import { useLibraryStore } from '@/stores/libraryStore';
import { useViewStore, type Layout } from '@/stores/viewStore';

import { Tip } from './Tip';

export function Toolbar({
  view,
  searchRef,
  onSettings,
  onFill,
  onJobs,
  onReports,
}: {
  view: LibraryView;
  searchRef: RefObject<HTMLInputElement | null>;
  onSettings(): void;
  onFill(): void;
  onJobs(): void;
  onReports(): void;
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
  const running = isRunning(job);
  const jobsLabel = running
    ? t('toolbar.jobsRunning')
    : job?.state === 'paused'
      ? t('toolbar.jobsPaused')
      : t('toolbar.jobs');

  return (
    <header className="toolbar">
      {/* Closing the library goes back to the start screen: a back
          affordance before the title, so it never wraps onto its own row. */}
      <Tip content={t('library.close')}>
        <Button
          variant="ghost"
          size="icon"
          className="-ml-1.5 text-cr-text-secondary"
          aria-label={t('library.close')}
          onClick={close}
        >
          <ChevronLeft aria-hidden />
        </Button>
      </Tip>
      <div className="toolbar-title">
        <h1 className="toolbar-name truncate" title={view.root}>
          {view.name}
        </h1>
        <span className="truncate text-caption text-cr-text-secondary">
          {view.profile.name}
        </span>
      </div>
      <div className="toolbar-search relative shrink-0">
        <Search
          aria-hidden
          className="pointer-events-none absolute left-2.5 top-1/2 size-4 -translate-y-1/2 text-cr-text-secondary"
        />
        <Input
          ref={searchRef}
          type="search"
          name="search"
          className="pl-8 pr-9"
          aria-label={t('toolbar.search')}
          placeholder={t('toolbar.searchShort')}
          value={filters.query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <Kbd
          aria-hidden
          className="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2"
        >
          /
        </Kbd>
      </div>
      <Button
        variant="outline"
        aria-expanded={filtersOpen}
        aria-controls="filter-panel"
        className="aria-expanded:border-primary aria-expanded:bg-cr-primary-subtle aria-expanded:text-cr-primary-subtle-fg"
        onClick={toggleFilters}
      >
        <SlidersHorizontal aria-hidden />
        <span className="sr-only">
          {active > 0
            ? t('toolbar.filtersActive', { n: active })
            : t('toolbar.filters')}
        </span>
        <span aria-hidden className="toolbar-label">
          {t('toolbar.filters')}
        </span>
        {active > 0 && (
          <Badge variant="primary" aria-hidden>
            {active}
          </Badge>
        )}
      </Button>
      <SegmentedControl
        aria-label={t('toolbar.layout')}
        value={layout}
        onValueChange={(v) => setLayout(v as Layout)}
      >
        {/* Icon-only segments: the name is the aria-label, the tooltip
            sits on a wrapper so it does not take the item's data-state. */}
        <Tip content={t('toolbar.grid')}>
          <span className="inline-flex">
            <SegmentedControlItem value="grid" aria-label={t('toolbar.grid')}>
              <LayoutGrid aria-hidden />
            </SegmentedControlItem>
          </span>
        </Tip>
        <Tip content={t('toolbar.list')}>
          <span className="inline-flex">
            <SegmentedControlItem value="list" aria-label={t('toolbar.list')}>
              <List aria-hidden />
            </SegmentedControlItem>
          </span>
        </Tip>
      </SegmentedControl>
      <span className="toolbar-spacer" />
      <Button variant="primary" onClick={onFill}>
        {t('toolbar.fill')}
      </Button>
      {/* Secondary actions: text buttons on wide windows; below 1180px they
          turn into icon buttons (.toolbar-icon) and the label moves to the
          tooltip (.toolbar-label), so the toolbar stays on one row. */}
      <Tip content={jobsLabel}>
        <Button variant="outline" onClick={onJobs}>
          {running ? (
            <Spinner size="sm" decorative />
          ) : (
            <ListChecks aria-hidden className="toolbar-icon" />
          )}
          <span className="toolbar-label">{jobsLabel}</span>
        </Button>
      </Tip>
      <Tip content={t('toolbar.reports')}>
        <Button variant="outline" onClick={onReports}>
          <FileText aria-hidden className="toolbar-icon" />
          <span className="toolbar-label">{t('toolbar.reports')}</span>
        </Button>
      </Tip>
      <Tip content={t('settings.open')}>
        <Button variant="outline" data-settings-opener onClick={onSettings}>
          <Settings aria-hidden className="toolbar-icon" />
          <span className="toolbar-label">{t('settings.open')}</span>
        </Button>
      </Tip>
      <LanguageSwitch
        size="md"
        className="toolbar-lang"
        valueClassName="toolbar-label"
      />
    </header>
  );
}
