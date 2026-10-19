import { ChevronRight } from 'lucide-react';
import { useContext, useMemo, useState, type ReactNode } from 'react';

import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible';
import { SystemIcon } from '@/components/system/SystemIcon';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { LocaleContext } from '@/i18n/core';

import { useT } from '@/i18n/useT';
import { cn } from '@/lib/utils';
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

  function toggle(path: string) {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  }

  const scopeButton = (
    target: Scope,
    label: string,
    count: number,
    icon?: ReactNode,
  ) => (
    <Button
      variant="ghost"
      className={cn(
        'h-auto min-h-control-sm w-full min-w-0 flex-1 justify-between gap-2 px-3 py-1 text-left font-normal',
        'aria-[current=true]:bg-cr-primary-subtle aria-[current=true]:font-semibold aria-[current=true]:text-cr-primary-subtle-fg',
      )}
      aria-current={sameScope(scope, target) ? 'true' : undefined}
      onClick={() => choose(target)}
    >
      <span className="flex min-w-0 items-center gap-1.5">
        {icon}
        <span className="truncate" title={label}>
          {label}
        </span>
      </span>
      <Badge mono className="bg-transparent px-0">
        {n.format(count)}
      </Badge>
    </Button>
  );

  function folder(node: FolderNode, depth: number): ReactNode {
    const indent = { paddingLeft: `${depth * 0.75}rem` };
    if (node.children.length === 0) {
      return (
        <li key={node.path}>
          <div className="flex items-center" style={indent}>
            <span className="size-7 shrink-0" aria-hidden="true" />
            {scopeButton(
              { kind: 'folder', path: node.path },
              node.name,
              node.count,
            )}
          </div>
        </li>
      );
    }
    const open = expanded.has(node.path);
    return (
      <li key={node.path}>
        <Collapsible open={open} onOpenChange={() => toggle(node.path)}>
          <div className="flex items-center" style={indent}>
            <CollapsibleTrigger asChild>
              <Button
                variant="ghost"
                size="icon-sm"
                className="group text-cr-text-secondary"
                aria-label={t(open ? 'sidebar.collapse' : 'sidebar.expand', {
                  name: node.name,
                })}
              >
                <ChevronRight
                  aria-hidden
                  className="transition-transform duration-fast group-data-[state=open]:rotate-90 motion-reduce:transition-none"
                />
              </Button>
            </CollapsibleTrigger>
            {scopeButton(
              { kind: 'folder', path: node.path },
              node.name,
              node.count,
            )}
          </div>
          <CollapsibleContent asChild>
            <ul className="m-0 list-none p-0">
              {node.children.map((c) => folder(c, depth + 1))}
            </ul>
          </CollapsibleContent>
        </Collapsible>
      </li>
    );
  }

  const panel = (id: Tab): ReactNode => {
    switch (id) {
      case 'all':
        return (
          <li>
            {scopeButton(
              { kind: 'all' },
              t('views.allRoms'),
              view.entries.length,
            )}
          </li>
        );
      case 'systems':
        return view.systems
          .filter((s) => s.roms > 0)
          .map((s) => (
            <li key={s.index}>
              {scopeButton(
                { kind: 'system', system: s.index },
                s.name,
                systemTotals.get(s.index) ?? 0,
                <SystemIcon systemId={s.system_id} size={16} />,
              )}
            </li>
          ));
      case 'date':
        return DATE_BUCKETS.map((b) => (
          <li key={b}>
            {scopeButton(
              { kind: 'date', bucket: b },
              t(`date.${b}`),
              buckets[b],
            )}
          </li>
        ));
      case 'folders':
        return tree.children.map((c) => folder(c, 0));
    }
  };

  return (
    <nav className="sidebar" aria-label={t('sidebar.label')}>
      <Tabs
        value={tab}
        onValueChange={(v) => activate(v as Tab)}
        className="min-h-0 flex-1 gap-0"
      >
        <TabsList
          aria-label={t('sidebar.label')}
          className="flex justify-between px-2 data-[orientation=horizontal]:gap-0"
        >
          {TABS.map((id) => (
            <TabsTrigger
              key={id}
              value={id}
              className="min-w-0 justify-center px-1.5 text-caption data-[orientation=horizontal]:after:inset-x-1"
            >
              <span className="truncate">{t(`views.${id}`)}</span>
            </TabsTrigger>
          ))}
        </TabsList>
        {TABS.map((id) => (
          <TabsContent
            key={id}
            value={id}
            className="min-h-0 flex-1 overflow-auto px-1 py-2"
          >
            <ul className="m-0 grid list-none gap-px p-0">{panel(id)}</ul>
          </TabsContent>
        ))}
      </Tabs>
    </nav>
  );
}
