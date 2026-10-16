import { useMemo, useState, type ReactNode } from 'react';

import { useT } from '@/i18n/useT';
import type { LibraryView } from '@/ipc/types';
import type { Facets } from '@/lib/filters';
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

export function Sidebar({
  view,
  facets,
  now,
}: {
  view: LibraryView;
  facets: Facets;
  now: number;
}) {
  const t = useT();
  const scope = useViewStore((s) => s.scope);
  const setScope = useViewStore((s) => s.setScope);
  const setSort = useViewStore((s) => s.setSort);
  const [tab, setTab] = useState<Tab>(() => tabOf(scope));
  const tree = useMemo(() => buildFolderTree(view.entries), [view.entries]);
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(() =>
    initialExpanded(buildFolderTree(view.entries)),
  );
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
      <span className="sidebar-count">{count.toLocaleString()}</span>
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
            className="sidebar-tab"
            onClick={() => {
              setTab(id);
              if (id === 'all') choose({ kind: 'all' });
            }}
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
                    facets.systems.get(s.index) ?? 0,
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
