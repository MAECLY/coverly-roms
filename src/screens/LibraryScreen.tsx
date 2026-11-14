import {
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react';

import { FillWizard } from '@/components/fill/FillWizard';
import { JobsPanel } from '@/components/fill/JobsPanel';
import { ReviewQueue } from '@/components/fill/ReviewQueue';
import { ContentFilters } from '@/components/library/ContentFilters';
import { DetailDrawer } from '@/components/library/DetailDrawer';
import { FilterPanel } from '@/components/library/FilterPanel';
import { OrphansList } from '@/components/library/OrphansList';
import { Sidebar } from '@/components/library/Sidebar';
import { StatusBar } from '@/components/library/StatusBar';
import { Toolbar } from '@/components/library/Toolbar';
import { ColumnsView } from '@/components/library/views/ColumnsView';
import { CoversView } from '@/components/library/views/CoversView';
import {
  DetailsView,
  type DetailsViewProps,
} from '@/components/library/views/DetailsView';
import { GalleryView } from '@/components/library/views/GalleryView';
import { MountsView } from '@/components/library/views/MountsView';
import { QuickLook } from '@/components/library/views/QuickLook';
import { ShelvesView } from '@/components/library/views/ShelvesView';
import type { ViewContent, ViewProps } from '@/components/library/views/types';
import { MiniPlayer } from '@/components/media/MiniPlayer';
import { ReportsPanel } from '@/components/reports/ReportsPanel';
import { Button } from '@/components/ui/button';
import { EmptyState } from '@/components/ui/empty-state';
import { UpdateBanner } from '@/components/update/UpdateBanner';
import { useViewShortcuts } from '@/hooks/useViewShortcuts';
import { useT } from '@/i18n/useT';
import type { Media } from '@/ipc/types';
import { hasContent as gamesHaveContent } from '@/lib/addonSummary';
import { ensureContentColumns } from '@/lib/detailsColumns';
import { isTyping } from '@/lib/keyboard';
import { newChildren } from '@/lib/seen';
import { inspectorDocked, type ViewId } from '@/lib/viewPrefs';
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
  const {
    visible,
    visibleGames,
    sections,
    facets,
    contentCounts,
    now,
    games,
    orphans,
    counts,
    shownCounts,
    byEntry,
  } = useVisibleEntries();
  const scope = useViewStore((s) => s.scope);
  // «Complementos sin juego base» is a list, whatever the view mode (D13).
  const orphansOpen = scope.kind === 'orphans' && orphans.length > 0;
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
  // Art slots: the slot the art workspace opens on (Enter on a slot).
  const [workspaceMedia, setWorkspaceMedia] = useState<Media | null>(null);
  const root = view?.root;
  // «Nuevo» (D18): read against this viewer's seen set once per open; the
  // set's baseline is kept for the session, so this is idempotent.
  const fresh = useMemo<ReadonlySet<string>>(
    () => (root === undefined ? new Set() : newChildren(root, games)),
    [root, games],
  );
  // A library flag, from every game: filters never change row heights.
  const contentLine = useMemo(() => gamesHaveContent(games), [games]);
  const loadPrefsFor = useViewStore((s) => s.loadFor);
  const setPrefs = useViewStore((s) => s.setPrefs);
  const contentRef = useRef<HTMLDivElement>(null);
  // The view on screen and the library it belongs to: a view that comes
  // with another library is an opening, not a switch.
  const prefsRoot = useViewStore((s) => s.root);
  const shown = useRef({ root: prefsRoot, view: prefs.view });

  // ⌘1–⌘6: views; ⌘+ / ⌘− / ⌘0 and ⌘/Ctrl + wheel: cover size (Covers, Shelves);
  // ⌘I: the docked inspector.
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

  // D19 (b): the first library with updates, DLC, revisions or demos adds
  // Versión and Contenido to the Details columns, once.
  const hasContent =
    counts.updates +
      counts.dlc +
      counts.moreRevisions +
      counts.demos +
      counts.orphans >
    0;
  useEffect(() => {
    if (root === undefined || prefsRoot !== root || !hasContent) return;
    const next = ensureContentColumns(prefs, hasContent);
    if (next !== prefs) setPrefs(next);
  }, [root, prefsRoot, hasContent, prefs, setPrefs]);

  // After a view switch (not on first mount, nor on opening a library) the
  // new view takes the focus: its name ("Columns view") is the
  // announcement, so there is no live region. The selected item stays the
  // active one and is scrolled into sight.
  useEffect(() => {
    const was = shown.current;
    shown.current = { root: prefsRoot, view: prefs.view };
    if (was.view === prefs.view || was.root !== prefsRoot) return;
    const content = contentRef.current;
    const viewRoot = content?.querySelector<HTMLElement>('[data-view-root]');
    if (!viewRoot) return;
    viewRoot.focus({ preventScroll: true });
    const { selectedId: id } = useSelectionStore.getState();
    if (id === null) return;
    const item = viewRoot.querySelector<HTMLElement>(
      `[id="entry-${String(id)}"]`,
    );
    // jsdom has no scrollIntoView.
    item?.scrollIntoView?.({ block: 'nearest', inline: 'nearest' });
  }, [prefsRoot, prefs.view]);

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
        // A docked inspector is a pane, not a popover: Escape leaves it.
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
  // Docked, the inspector follows the selection; otherwise Enter opens it.
  const docked = inspectorDocked(prefs);

  const onOpen = (id: number) => {
    setWorkspaceMedia(null);
    select(id);
    openDrawer();
  };
  // Enter on (or a click in) an Art slots slot: straight to the fix.
  const onOpenArt = (id: number, media: Media) => {
    setWorkspaceMedia(media);
    select(id);
    openDrawer();
  };
  // A click on a cover's add-on summary: the panel, at Versión (§8.2).
  const onOpenVersion = (id: number) => {
    setWorkspaceMedia(null);
    select(id);
    openDrawer('version');
  };
  // A click on an update or DLC in the Galería tray: the panel, at it (§8.3).
  const onOpenChild = (id: number, childIndex: number) => {
    setWorkspaceMedia(null);
    select(id);
    openDrawer(`child-${childIndex}`);
  };
  const viewContent: ViewContent = {
    byEntry,
    line: contentLine,
    fresh,
    onOpenVersion,
    onOpenChild,
  };
  const list = {
    entries: visible,
    systems: view.systems,
    generation: view.generation,
    selectedId,
    onSelect: (id: number) => {
      // The keyboard moved on: the next game opens on its own first slot.
      setWorkspaceMedia(null);
      select(id);
    },
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
  } else if (orphansOpen) {
    content = (
      <OrphansList
        orphans={visibleGames}
        systems={view.systems}
        byEntry={byEntry}
      />
    );
  } else {
    content = renderView(
      prefs.view,
      {
        ...list,
        sections,
        prefs,
        onPreview: setPreviewId,
        onOpenArt,
        content: viewContent,
      },
      { sort, onSort: toggleSort, setPrefs },
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
        onDuplicates={() => useNavStore.getState().openDuplicates(view.root)}
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
        <Sidebar
          key={view.generation}
          view={view}
          games={games}
          orphans={orphans.length}
          now={now}
        />
        <main className="library-main">
          <UpdateBanner />
          {/* «Mostrar juegos que» above the views (D14, Q5); the orphans
              list is add-ons, not games, so it has no chips. */}
          {!orphansOpen && (
            <ContentFilters
              counts={contentCounts}
              shown={visibleGames.length}
            />
          )}
          {filtersOpen && <FilterPanel view={view} facets={facets} />}
          <div ref={contentRef} className="library-content">
            {content}
          </div>
        </main>
        {(docked || drawerOpen) && selected && (
          <DetailDrawer
            view={view}
            entry={selected}
            game={byEntry.get(selected.id)}
            docked={docked}
            workspaceMedia={workspaceMedia}
            fresh={fresh}
            onClose={() => {
              // Docked, the close button undocks the pane. Either way the
              // button that held the focus is gone: the view takes it back.
              if (docked) setPrefs({ inspector: false });
              closeDrawer();
              document
                .querySelector<HTMLElement>('.library-content [data-view-root]')
                ?.focus();
            }}
          />
        )}
      </div>
      {/* With the panel closed the mini player folds into a strip; the
          panel docks its own (extras D19). */}
      {!((docked || drawerOpen) && selected) && <MiniPlayer variant="strip" />}
      <StatusBar
        total={counts}
        shown={orphansOpen ? counts : shownCounts}
        filtered={!orphansOpen && visibleGames.length !== counts.games}
        games={orphansOpen ? games : visibleGames}
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
          games={byEntry}
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

/** The library view for `prefs.view`; Details also sorts and sizes columns. */
function renderView(
  id: ViewId,
  props: ViewProps,
  details: Pick<DetailsViewProps, 'sort' | 'onSort' | 'setPrefs'>,
): ReactNode {
  switch (id) {
    case 'covers':
      return <CoversView {...props} />;
    case 'details':
      return <DetailsView {...props} {...details} />;
    case 'columns':
      return <ColumnsView {...props} />;
    case 'gallery':
      return <GalleryView {...props} />;
    case 'mounts':
      return <MountsView {...props} />;
    case 'shelves':
      return <ShelvesView {...props} />;
  }
}
