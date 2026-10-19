import {
  ChevronDown,
  Columns3,
  GalleryHorizontalEnd,
  GalleryThumbnails,
  LayoutGrid,
  Library,
  Table2,
  type LucideIcon,
} from 'lucide-react';
import { Fragment } from 'react';

import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuShortcut,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Kbd, KbdGroup } from '@/components/ui/kbd';
import {
  SegmentedControl,
  SegmentedControlItem,
} from '@/components/ui/toggle-group';
import { useT } from '@/i18n/useT';
import { modKeyLabel } from '@/lib/keyboard';
import { VIEW_IDS, type ViewId } from '@/lib/viewPrefs';
import { useViewStore } from '@/stores/viewStore';

import { Tip } from './Tip';

const VIEW_ICONS: Record<ViewId, LucideIcon> = {
  covers: LayoutGrid,
  details: Table2,
  columns: Columns3,
  gallery: GalleryThumbnails,
  mounts: GalleryHorizontalEnd,
  shelves: Library,
};

/** Finder's four (Covers … Gallery) end here; Coverly's two follow. */
const FINDER_VIEWS = 4;

function isViewId(v: string): v is ViewId {
  return (VIEW_IDS as readonly string[]).includes(v);
}

/**
 * The six library views (spec §B.6). Wide windows get one segmented control
 * with a hairline between Finder's views and Coverly's; below 1180px the CSS
 * swaps it for a single `View ▾` menu. Both stay mounted and read the same
 * `prefs.view`, so whichever is shown is always in step.
 */
export function ViewSwitcher() {
  const t = useT();
  const view = useViewStore((s) => s.prefs.view);
  const setView = useViewStore((s) => s.setView);
  const mod = modKeyLabel();
  const shortcut = (n: number) => (mod === '⌘' ? `⌘${n}` : `Ctrl+${n}`);
  // The visible shortcut is a tooltip or an aria-hidden hint; this is what
  // assistive tech reads. Both modifiers work (spec §B.6).
  const keys = (n: number) => `Meta+${n} Control+${n}`;
  const choose = (v: string) => {
    if (isViewId(v)) setView(v);
  };
  const Current = VIEW_ICONS[view];

  return (
    <div className="view-switcher">
      <SegmentedControl
        aria-label={t('layouts.label')}
        className="view-switcher__segments"
        value={view}
        onValueChange={choose}
      >
        {VIEW_IDS.map((id, i) => {
          const Icon = VIEW_ICONS[id];
          const name = t(`layouts.${id}`);
          return (
            <Fragment key={id}>
              {i === FINDER_VIEWS && (
                <span aria-hidden className="view-switcher__rule" />
              )}
              {/* Icon-only segment: the name is the aria-label; the tooltip
                  sits on a wrapper so it does not take the item's state. */}
              <Tip
                content={
                  <span className="inline-flex items-center gap-2">
                    {name}
                    <KbdGroup>
                      <Kbd>{mod}</Kbd>
                      <Kbd>{i + 1}</Kbd>
                    </KbdGroup>
                  </span>
                }
              >
                <span className="inline-flex">
                  <SegmentedControlItem
                    value={id}
                    aria-label={name}
                    aria-keyshortcuts={keys(i + 1)}
                  >
                    <Icon aria-hidden />
                  </SegmentedControlItem>
                </span>
              </Tip>
            </Fragment>
          );
        })}
      </SegmentedControl>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button variant="outline" className="view-switcher__menu">
            <Current aria-hidden />
            {t('layouts.label')}
            <span className="sr-only">, {t(`layouts.${view}`)}</span>
            <ChevronDown aria-hidden className="text-cr-text-secondary" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start">
          <DropdownMenuRadioGroup value={view} onValueChange={choose}>
            {VIEW_IDS.map((id, i) => {
              const Icon = VIEW_ICONS[id];
              return (
                <Fragment key={id}>
                  {i === FINDER_VIEWS && <DropdownMenuSeparator />}
                  <DropdownMenuRadioItem
                    value={id}
                    aria-keyshortcuts={keys(i + 1)}
                  >
                    <Icon aria-hidden className="size-4" />
                    {t(`layouts.${id}`)}
                    <DropdownMenuShortcut>
                      {shortcut(i + 1)}
                    </DropdownMenuShortcut>
                  </DropdownMenuRadioItem>
                </Fragment>
              );
            })}
          </DropdownMenuRadioGroup>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}
