import { useVirtualizer } from '@tanstack/react-virtual';
import {
  useCallback,
  useContext,
  useId,
  useMemo,
  useRef,
  useState,
  type MouseEvent,
  type WheelEvent,
} from 'react';

import { SystemLabel } from '@/components/system/SystemLabel';
import { Button } from '@/components/ui/button';
import { SlotMark, type SlotState } from '@/components/ui/slot-mark';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { useActiveRange } from '@/hooks/useActiveRange';
import { useArtUrl } from '@/hooks/useArtUrl';
import { useDebouncedValue } from '@/hooks/useDebouncedValue';
import { useElementSize } from '@/hooks/useElementSize';
import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import { MEDIA, type ArtMark, type EntryView, type Media } from '@/ipc/types';
import { artKey, peekArtUrl } from '@/lib/artCache';
import { formatSize } from '@/lib/format';
import { sectionLabel } from '@/lib/grouping';
import { cn } from '@/lib/utils';
import { useViewStore } from '@/stores/viewStore';

import { ArtThumb } from '../ArtThumb';
import { entryIdOf, type ViewProps } from './types';
import { useViewKeys } from './useViewKeys';

/** Each filmstrip cell: a 64px thumbnail and its gutter (spec §B.4.4). */
export const FILMSTRIP_CELL_W = 72;

/** The large art is asked for once the selection has stayed still this long. */
export const GALLERY_SETTLE_MS = 120;

const SLOT_STATE: Record<ArtMark, SlotState> = {
  present: 'filled',
  missing: 'missing',
  unsupported: 'unsupported',
};

function isMedia(value: string): value is Media {
  return (MEDIA as readonly string[]).includes(value);
}

/** The large art of `entry`, or its empty slot. Only asked for when `load`. */
function GalleryArt({
  generation,
  entry,
  media,
  load,
}: {
  generation: number;
  entry: EntryView;
  media: Media;
  load: boolean;
}) {
  const t = useT();
  const mark = entry.art[media];
  const fetched = useArtUrl(
    generation,
    entry.id,
    media,
    load && mark === 'present',
  );
  // Art already in the cache shows at once, settled or not: going back to a
  // game seen a moment ago never blanks its slot, and asks for nothing.
  const url =
    fetched ??
    (mark === 'present'
      ? peekArtUrl(artKey(generation, entry.id, media))
      : null);
  const mediaName = t(`art.media.${media}`);
  return (
    <div
      className={cn(
        'gallery__art',
        media === 'cover' ? 'gallery__art--cover' : 'gallery__art--wide',
      )}
      data-slot-state={SLOT_STATE[mark]}
    >
      {url ? (
        // Keyed by URL so each image fades in on its own (reduced motion: no fade).
        <img
          key={url}
          src={url}
          alt={`${entry.name}, ${mediaName}`}
          draggable={false}
          decoding="async"
          className="gallery__img"
        />
      ) : mark === 'present' ? (
        // Waiting for the selection to settle, or loading: a filled slot.
        // The image that replaces it carries the name.
        <div
          aria-hidden="true"
          className="art-placeholder gallery__placeholder"
        />
      ) : (
        <div
          role="img"
          aria-label={`${entry.name}, ${
            mark === 'missing'
              ? t(`art.none.${media}`)
              : t('art.status', {
                  media: mediaName,
                  state: t('art.mark.unsupported'),
                })
          }`}
          className="art-placeholder gallery__placeholder"
        >
          {mark === 'missing' && (
            <>
              <SlotMark state="missing" size={48} />
              <span>{t(`art.none.${media}`)}</span>
            </>
          )}
          {mark === 'unsupported' && <SlotMark state="unsupported" size={48} />}
        </div>
      )}
    </div>
  );
}

/**
 * Gallery (⌘4): one game's art large, with its facts beside it, above a
 * filmstrip of the whole list. ←/→ move along the strip (↑/↓ do nothing);
 * the art tabs, the strip and the facts are reached with Tab in that order,
 * so the view never traps the keyboard. The large image is asked for only
 * once the selection has stayed still for 120 ms, so fast arrowing never
 * queues up images.
 */
export function GalleryView({
  entries,
  sections,
  prefs,
  systems,
  generation,
  selectedId,
  onSelect,
  onOpen,
  onPreview,
}: ViewProps) {
  const t = useT();
  const locale = useContext(LocaleContext);
  const baseId = useId();
  const [media, setMedia] = useState<Media>(prefs.art);
  const [ref, size, element] = useElementSize<HTMLDivElement>();
  const stripRef = useRef<HTMLDivElement | null>(null);
  const setStrip = useCallback(
    (el: HTMLDivElement | null) => {
      stripRef.current = el;
      ref(el);
    },
    [ref],
  );
  const pageItems = Math.max(1, Math.floor(size.width / FILMSTRIP_CELL_W));
  const indexOf = useMemo(
    () => new Map(entries.map((e, i) => [e.id, i])),
    [entries],
  );
  const selectedIndex =
    selectedId === null ? -1 : (indexOf.get(selectedId) ?? -1);
  const grouped = prefs.groupBy !== 'none';
  // Entry index → the section it starts (markers and descriptions).
  const starts = useMemo(() => {
    const map = new Map<number, number>();
    if (!grouped) return map;
    sections.forEach((s, i) => {
      if (s.count > 0) map.set(s.start, i);
    });
    return map;
  }, [sections, grouped]);
  const rangeExtractor = useActiveRange(selectedIndex, null);
  const getItemKey = useCallback((i: number) => entries[i]?.id ?? i, [entries]);
  // eslint-disable-next-line react-hooks/incompatible-library -- TanStack Virtual is not memoisable by the React Compiler
  const virtualizer = useVirtualizer({
    horizontal: true,
    count: entries.length,
    getScrollElement: () => element.current,
    estimateSize: () => FILMSTRIP_CELL_W,
    // What is on screen plus one screen in all, half on each side (§B.7).
    overscan: Math.ceil(pageItems / 2),
    getItemKey,
    rangeExtractor,
    initialRect: size,
  });

  const settledId = useDebouncedValue(selectedId, GALLERY_SETTLE_MS);
  const settled = settledId === selectedId;

  const keys = useViewKeys({
    entries,
    sections,
    selectedIndex,
    onSelect,
    onOpen,
    onPreview,
    scrollToIndex: (i) => virtualizer.scrollToIndex(i, { align: 'auto' }),
    layout: String(size.width),
    move: (key) => {
      const last = entries.length - 1;
      if (last < 0) return null;
      const from = selectedIndex;
      const clamp = (i: number) => Math.min(last, Math.max(0, i));
      switch (key) {
        case 'ArrowRight':
          return from < 0 ? 0 : clamp(from + 1);
        case 'ArrowLeft':
          return from < 0 ? 0 : clamp(from - 1);
        case 'Home':
          return 0;
        case 'End':
          return last;
        case 'PageDown':
          return from < 0 ? 0 : clamp(from + pageItems);
        case 'PageUp':
          return from < 0 ? 0 : clamp(from - pageItems);
        default:
          // ↑/↓ do nothing: the strip has one dimension.
          return null;
      }
    },
  });

  // A plain mouse wheel scrolls the strip sideways (it has no vertical
  // scroll), so Windows and Linux mice can browse it without Shift.
  function onStripWheel(ev: WheelEvent<HTMLDivElement>) {
    if (ev.shiftKey || Math.abs(ev.deltaX) >= Math.abs(ev.deltaY)) return;
    const scale =
      ev.deltaMode === 1
        ? FILMSTRIP_CELL_W
        : ev.deltaMode === 2
          ? size.width
          : 1;
    ev.currentTarget.scrollLeft += ev.deltaY * scale;
  }

  function onStripClick(ev: MouseEvent<HTMLDivElement>) {
    const id = entryIdOf(ev.target);
    if (id !== null) onSelect(id);
  }

  const shown = selectedIndex >= 0 ? entries[selectedIndex] : undefined;
  const system = shown ? systems[shown.system] : undefined;
  const facts = shown
    ? [
        ...(shown.regions.length > 0
          ? [shown.regions.map((r) => t(`regions.${r}`)).join(', ')]
          : []),
        formatSize(shown.size, locale),
        `.${shown.ext}`,
      ]
    : [];

  return (
    <div
      role="group"
      aria-label={t('layouts.announce', { view: t('layouts.gallery') })}
      data-view-root=""
      tabIndex={-1}
      className="gallery"
      onFocus={(ev) => {
        if (ev.target === ev.currentTarget) stripRef.current?.focus();
      }}
    >
      <Tabs
        value={media}
        onValueChange={(v) => {
          if (isMedia(v)) setMedia(v);
        }}
        className="gallery__stage"
      >
        <TabsList aria-label={t('layouts.art')} className="gallery__tabs">
          {MEDIA.map((m) => (
            <TabsTrigger key={m} value={m}>
              {t(`art.media.${m}`)}
            </TabsTrigger>
          ))}
        </TabsList>
        {/* Not a Tab stop: Tab goes from the art tabs to the filmstrip. */}
        <TabsContent value={media} tabIndex={-1} className="gallery__frame">
          {shown ? (
            <GalleryArt
              generation={generation}
              entry={shown}
              media={media}
              load={settled}
            />
          ) : (
            <p className="gallery__empty">{t('layouts.previewEmpty')}</p>
          )}
        </TabsContent>
      </Tabs>

      <div
        ref={setStrip}
        role="listbox"
        aria-label={t('layouts.filmstrip')}
        aria-orientation="horizontal"
        aria-activedescendant={shown ? `entry-${shown.id}` : undefined}
        tabIndex={0}
        className="gallery__strip"
        onKeyDown={keys}
        onClick={onStripClick}
        onWheel={onStripWheel}
        onDoubleClick={(ev) => {
          const id = entryIdOf(ev.target);
          if (id !== null) onOpen(id);
        }}
      >
        <div
          style={{ width: virtualizer.getTotalSize(), position: 'relative' }}
          className="gallery__track"
        >
          {virtualizer.getVirtualItems().map((item) => {
            const entry = entries[item.index];
            if (!entry) return null;
            const sectionIndex = starts.get(item.index);
            const section =
              sectionIndex === undefined ? undefined : sections[sectionIndex];
            const groupName = section ? sectionLabel(section, t) : undefined;
            const descId = `${baseId}-g${sectionIndex}`;
            const selected = entry.id === selectedId;
            return (
              <div
                key={item.key}
                style={{
                  transform: `translateX(${item.start}px)`,
                  width: FILMSTRIP_CELL_W,
                }}
                className="gallery__cell"
                data-group-start={section ? 'true' : undefined}
              >
                {section && (
                  <>
                    {/* The shelf-rule marker; its name is the option's description. */}
                    <span
                      aria-hidden="true"
                      className="gallery__marker"
                      title={groupName}
                    />
                    <span id={descId} hidden>
                      {groupName}
                    </span>
                  </>
                )}
                <div
                  id={`entry-${entry.id}`}
                  role="option"
                  aria-selected={selected}
                  aria-setsize={entries.length}
                  aria-posinset={item.index + 1}
                  aria-label={entry.name}
                  aria-describedby={section ? descId : undefined}
                  data-entry-id={entry.id}
                  className="gallery__thumb"
                  title={entry.name}
                >
                  <ArtThumb
                    generation={generation}
                    entry={entry}
                    media="cover"
                    placeholder=""
                  />
                </div>
              </div>
            );
          })}
        </div>
      </div>

      <section aria-label={t('layouts.pane.preview')} className="gallery__meta">
        {shown ? (
          <>
            <h3 className="gallery__title">{shown.name}</h3>
            <SystemLabel
              systemId={system?.system_id ?? null}
              name={system?.name ?? ''}
              size={20}
            />
            <p className="gallery__facts">
              {facts.map((f, i) => (
                <span key={i} className="gallery__fact">
                  {f}
                </span>
              ))}
            </p>
            <ul className="gallery__slots">
              {MEDIA.map((m) => {
                const mark = shown.art[m];
                return (
                  <li key={m} className="gallery__slot" data-media={m}>
                    <SlotMark state={SLOT_STATE[mark]} size={16} />
                    <span className="gallery__slot-name">
                      {t(`art.media.${m}`)}
                    </span>
                    <span className="gallery__slot-state">
                      {t(`art.mark.${mark}`)}
                    </span>
                  </li>
                );
              })}
            </ul>
            <Button
              variant="secondary"
              size="md"
              className="gallery__fill"
              onClick={() => {
                onSelect(shown.id);
                useViewStore.getState().setOverlay('wizard');
              }}
            >
              {t('layouts.fillThisGame')}
            </Button>
          </>
        ) : (
          <p className="gallery__empty">{t('layouts.previewEmpty')}</p>
        )}
      </section>
    </div>
  );
}
