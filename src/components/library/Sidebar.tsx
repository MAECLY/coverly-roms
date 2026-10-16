import {
  useContext,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
} from 'react';

import { LocaleContext } from '@/i18n/core';

import { useT } from '@/i18n/useT';
import type { LibraryView } from '@/ipc/types';
import { buildFolderTree, type FolderNode } from '@/lib/folderTree';
import {
  DATE_BUCKETS,
  dateBucket,
  sameScope,
  type DateBucket,
  type Scope,
} from '@/lib/scope';
import { useViewStore } from '@/stores/viewStore';

type Tab = 'folders' | 'systems' | 'date' | 'all';
const TABS: readonly Tab[] = ['folders', 'systems', 'date', 'all'];

function tabOf(scope: Scope): Tab {
  switch (scope.kind) {
    case 'folder':
      return 'folders';
    case 'system':
      return 'systems';
    case 'date':
      return 'date';
    case 'all':
      return 'all';
  }
}

/** Folders on the single-child chain from the root start expanded. */
function initialExpanded(root: FolderNode): Set<string> {
  const open = new Set<string>();
  let node = root;
  while (node.children.length === 1) {
    const only = node.children[0]!;
    open.add(only.path);
    node = only;
  }
  return open;
}

/**
 * Library navigation. Every count is an unfiltered library total (how many
 * ROMs live in that folder, system or date bucket); the search and filters
 * only change the status bar and the filter panel facets.
 */
export function Sidebar({ view, now }: { view: LibraryView; now: number }) {
  const t = useT();
  const locale = useContext(LocaleContext);
  const n = useMemo(() => new Intl.NumberFormat(locale), [locale]);
  const tabRefs = useRef(new Map<Tab, HTMLButtonElement>());
  const scope = useViewStore((s) => s.scope);
  const setScope = useViewStore((s) => s.setScope);
  const setSort = useViewStore((s) => s.setSort);
  const [tab, setTab] = useState<Tab>(() => tabOf(scope));
  const tree = useMemo(() => buildFolderTree(view.entries), [view.entries]);
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(() =>
    initialExpanded(buildFolderTree(view.entries)),
  );
  const systemTotals = useMemo(() => {
    const counts = new Map<number, number>();
    for (const e of view.entries)
      counts.set(e.system, (counts.get(e.system) ?? 0) + 1);
    return counts;
  }, [view.entries]);
  const buckets = useMemo(() => {
    const counts: Record<DateBucket, number> = {
      week: 0,
      month: 0,
      year: 0,
      older: 0,
    };
    for (const e of view.entries) counts[dateBucket(e.mtime, now)] += 1;
    return counts;
  }, [view.entries, now]);

  function choose(target: Scope) {
    setScope(target);
    if (target.kind === 'date') setSort({ key: 'mtime', dir: 'desc' });
  }

  function activate(id: Tab) {
    setTab(id);
    if (id === 'all') choose({ kind: 'all' });
  }

  /** WAI-ARIA tabs: arrows (and Home/End) move and activate the tab. */
  function onTabKey(ev: KeyboardEvent<HTMLButtonElement>, id: Tab) {
    const at = TABS.indexOf(id);
    let next: number;
    if (ev.key === 'ArrowRight') next = (at + 1) % TABS.length;
    else if (ev.key === 'ArrowLeft')
      next = (at - 1 + TABS.length) % TABS.length;
    else if (ev.key === 'Home') next = 0;
    else if (ev.key === 'End') next = TABS.length - 1;
    else return;
    ev.preventDefault();
    const target = TABS[next]!;
    activate(target);
    tabRefs.current.get(target)?.focus();
  }

  function toggle(path: string) {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  }

  const scopeButton = (target: Scope, label: string, count: number) => (
    <button
      type="button"
      className="sidebar-item"
      aria-current={sameScope(scope, target) ? 'true' : undefined}
      onClick={() => choose(target)}
    >
      <span className="truncate" title={label}>
        {label}
      </span>
      <span className="sidebar-count">{n.format(count)}</span>
    </button>
  );

  function folder(node: FolderNode, depth: number): ReactNode {
    const open = expanded.has(node.path);
    return (
      <li key={node.path}>
        <div
          className="sidebar-folder"
          style={{ paddingLeft: `${depth * 0.75}rem` }}
        >
          {node.children.length > 0 ? (
            <button
              type="button"
              className="tree-toggle"
              aria-expanded={open}
              aria-label={t(open ? 'sidebar.collapse' : 'sidebar.expand', {
                name: node.name,
              })}
              onClick={() => toggle(node.path)}
            >
              {open ? '▾' : '▸'}
            </button>
          ) : (
            <span className="tree-toggle" aria-hidden="true" />
          )}
          {scopeButton(
            { kind: 'folder', path: node.path },
            node.name,
            node.count,
          )}
        </div>
        {open && node.children.length > 0 && (
          <ul className="sidebar-list">
            {node.children.map((c) => folder(c, depth + 1))}
          </ul>
        )}
      </li>
    );
  }

  return (
    <nav className="sidebar" aria-label={t('sidebar.label')}>
      <div
        role="tablist"
        aria-label={t('sidebar.label')}
        className="sidebar-tabs"
      >
        {TABS.map((id) => (
          <button
            key={id}
            type="button"
            role="tab"
            id={`tab-${id}`}
            aria-selected={tab === id}
            aria-controls={`panel-${id}`}
            tabIndex={tab === id ? 0 : -1}
            ref={(el) => {
              if (el) tabRefs.current.set(id, el);
              else tabRefs.current.delete(id);
            }}
            className="sidebar-tab"
            onClick={() => activate(id)}
            onKeyDown={(ev) => onTabKey(ev, id)}
          >
            {t(`views.${id}`)}
          </button>
        ))}
      </div>
      <div
        role="tabpanel"
        id={`panel-${tab}`}
        aria-labelledby={`tab-${tab}`}
        className="sidebar-panel"
      >
        <ul className="sidebar-list">
          {tab === 'all' && (
            <li>
              {scopeButton(
                { kind: 'all' },
                t('views.allRoms'),
                view.entries.length,
              )}
            </li>
          )}
          {tab === 'systems' &&
            view.systems
              .filter((s) => s.roms > 0)
              .map((s) => (
                <li key={s.index}>
                  {scopeButton(
                    { kind: 'system', system: s.index },
                    s.name,
                    systemTotals.get(s.index) ?? 0,
                  )}
                </li>
              ))}
          {tab === 'date' &&
            DATE_BUCKETS.map((b) => (
              <li key={b}>
                {scopeButton(
                  { kind: 'date', bucket: b },
                  t(`date.${b}`),
                  buckets[b],
                )}
              </li>
            ))}
          {tab === 'folders' && tree.children.map((n) => folder(n, 0))}
        </ul>
      </div>
    </nav>
  );
}
