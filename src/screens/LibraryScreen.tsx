import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
} from 'react';

import { FillWizard } from '@/components/fill/FillWizard';
import { JobsPanel } from '@/components/fill/JobsPanel';
import { ReviewQueue } from '@/components/fill/ReviewQueue';
import { CoverGrid } from '@/components/library/CoverGrid';
import { DetailDrawer } from '@/components/library/DetailDrawer';
import { EntryTable } from '@/components/library/EntryTable';
import { FilterPanel } from '@/components/library/FilterPanel';
import { Sidebar } from '@/components/library/Sidebar';
import { StatusBar } from '@/components/library/StatusBar';
import { Toolbar } from '@/components/library/Toolbar';
import { QuickLook } from '@/components/library/views/QuickLook';
import { ReportsPanel } from '@/components/reports/ReportsPanel';
import { Button } from '@/components/ui/button';
import { EmptyState } from '@/components/ui/empty-state';
import { UpdateBanner } from '@/components/update/UpdateBanner';
import { useViewShortcuts } from '@/hooks/useViewShortcuts';
import { useT } from '@/i18n/useT';
import { isTyping } from '@/lib/keyboard';
import { useFilterStore } from '@/stores/filterStore';
import { useJobStore } from '@/stores/jobStore';
import { useLibraryStore } from '@/stores/libraryStore';
import { useNavStore } from '@/stores/navStore';
import { useSelectionStore } from '@/stores/selectionStore';
import { useVisibleEntries } from '@/stores/useVisibleEntries';
import { useViewStore } from '@/stores/viewStore';

export function LibraryScreen() {
  const t = useT();
  const view = useLibraryStore((s) => s.view);
  const { visible, sections, facets, now } = useVisibleEntries();
  const layout = useViewStore((s) => s.layout);
  const prefs = useViewStore((s) => s.prefs);
  const filtersOpen = useViewStore((s) => s.filtersOpen);
  const drawerOpen = useViewStore((s) => s.drawerOpen);
  const sort = useViewStore((s) => s.sort);
  const toggleSort = useViewStore((s) => s.toggleSort);
  const openDrawer = useViewStore((s) => s.openDrawer);
  const closeDrawer = useViewStore((s) => s.closeDrawer);
  const overlay = useViewStore((s) => s.overlay);
  const setOverlay = useViewStore((s) => s.setOverlay);
  const closeOverlay = useViewStore((s) => s.closeOverlay);
  const selectedId = useSelectionStore((s) => s.selectedId);
  const select = useSelectionStore((s) => s.select);
  const clearFilters = useFilterStore((s) => s.clear);
  const setQuery = useFilterStore((s) => s.setQuery);
  const searchRef = useRef<HTMLInputElement>(null);
  // Space in a view: the entry shown large in Quick look, if any.
  const [previewId, setPreviewId] = useState<number | null>(null);
  const root = view?.root;
  const loadPrefsFor = useViewStore((s) => s.loadFor);
  const setPrefs = useViewStore((s) => s.setPrefs);

  // ⌘+ / ⌘− / ⌘0 and ⌘/Ctrl + wheel: cover size (Covers, Shelves).
  useViewShortcuts({
    prefs,
    setPrefs,
    enabled: overlay === 'none' && previewId === null,
  });

  // Each library opens with its own view prefs (or the last ones used).
  // Before paint, so the last library's view never flashes or mounts.
  useLayoutEffect(() => {
    if (root !== undefined) loadPrefsFor(root);
  }, [root, loadPrefsFor]);

  // Job events while this screen lives (shared with App's subscription).
  useEffect(() => {
    let alive = true;
    let off: (() => void) | undefined;
    void useJobStore
      .getState()
      .connect()
      .then((unsubscribe) => {
        if (alive) off = unsubscribe;
        else unsubscribe();
      });
    return () => {
      alive = false;
      off?.();
    };
  }, []);

  useEffect(() => {
    function onKey(ev: KeyboardEvent) {
      // A modal owns the keyboard; the search sits behind it. A Radix layer
      // (alert dialog, tooltip) that already handled Escape marks it
      // defaultPrevented: the drawer stays open.
      if (overlay !== 'none' || ev.defaultPrevented) return;
      const find = ev.key === 'f' && (ev.metaKey || ev.ctrlKey);
      if (find || (ev.key === '/' && !isTyping(ev.target))) {
        ev.preventDefault();
        searchRef.current?.focus();
      } else if (ev.key === 'Escape') {
        closeDrawer();
      }
    }
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [closeDrawer, overlay]);

  if (!view) return null;

  // Entry ids are indexes into `view.entries`.
  const selected =
    selectedId === null ? null : (view.entries[selectedId] ?? null);

  const onOpen = (id: number) => {
    select(id);
    openDrawer();
  };
  const list = {
    entries: visible,
    systems: view.systems,
    generation: view.generation,
    selectedId,
    onSelect: select,
    onOpen,
  };

  let content: ReactNode;
  if (view.entries.length === 0) {
    content = (
      <EmptyState
        className="h-full justify-center"
        title={t('library.empty')}
        description={t('library.emptyHint', { n: view.warnings.length })}
      />
    );
  } else if (visible.length === 0) {
    content = (
      <EmptyState
        className="h-full justify-center"
        title={t('library.noMatch')}
        actions={
          <Button
            variant="outline"
            onClick={() => {
              clearFilters();
              setQuery('');
            }}
          >
            {t('filters.clear')}
          </Button>
        }
      />
    );
  } else if (layout === 'grid') {
    content = (
      <CoverGrid
        {...list}
        sections={sections}
        prefs={prefs}
        onPreview={setPreviewId}
      />
    );
  } else {
    content = (
      <EntryTable
        {...list}
        sections={sections}
        onPreview={setPreviewId}
        sort={sort}
        onSort={toggleSort}
      />
    );
  }

  return (
    <div className="library-shell">
      <Toolbar
        view={view}
        searchRef={searchRef}
        onSettings={() => useNavStore.getState().openSettings()}
        onFill={() => setOverlay('wizard')}
        onJobs={() => setOverlay('jobs')}
        onReports={() => setOverlay('reports')}
        onOrganizeLoose={() =>
          useNavStore.getState().openOrganize(view.root, {
            layout: view.profile.id,
            mode: 'move',
            locked: true,
            flat: view.profile.fitted,
          })
        }
      />
      <div className="library-body">
        <Sidebar key={view.generation} view={view} now={now} />
        <main className="library-main">
          <UpdateBanner />
          {filtersOpen && <FilterPanel view={view} facets={facets} />}
          <div className="library-content">{content}</div>
        </main>
        {drawerOpen && selected && (
          <DetailDrawer view={view} entry={selected} onClose={closeDrawer} />
        )}
      </div>
      <StatusBar
        visible={visible}
        total={view.entries.length}
        scanMs={view.scan_ms}
      />
      <QuickLook
        entries={visible}
        systems={view.systems}
        generation={view.generation}
        media={prefs.art}
        previewId={previewId}
        onSelect={(id) => {
          select(id);
          setPreviewId(id);
        }}
        onClose={() => setPreviewId(null)}
      />
      {overlay === 'wizard' && (
        <FillWizard
          view={view}
          visible={visible}
          selectedId={selectedId}
          onClose={closeOverlay}
        />
      )}
      {overlay === 'review' && (
        <ReviewQueue view={view} onClose={() => setOverlay('wizard')} />
      )}
      {overlay === 'jobs' && <JobsPanel view={view} onClose={closeOverlay} />}
      {overlay === 'reports' && (
        <ReportsPanel view={view} onClose={closeOverlay} />
      )}
    </div>
  );
}
