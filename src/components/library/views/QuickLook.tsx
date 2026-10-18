import { ChevronLeft, ChevronRight } from 'lucide-react';
import { useMemo, useRef, type KeyboardEvent } from 'react';

import { SystemLabel } from '@/components/system/SystemLabel';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { useArtUrl } from '@/hooks/useArtUrl';
import { useT } from '@/i18n/useT';
import type { EntryView, Media, SystemView } from '@/ipc/types';

export interface QuickLookProps {
  entries: readonly EntryView[];
  systems: readonly SystemView[];
  generation: number;
  /** The art the current view shows (`prefs.art`). */
  media: Media;
  /** The entry shown; null (or one not in `entries`) keeps it closed. */
  previewId: number | null;
  /** ←/→ stepped to another entry: it becomes the selection too. */
  onSelect(id: number): void;
  onClose(): void;
}

function Art({
  generation,
  entry,
  media,
}: {
  generation: number;
  entry: EntryView;
  media: Media;
}) {
  const t = useT();
  const mark = entry.art[media];
  const url = useArtUrl(generation, entry.id, media, mark === 'present');
  const shape =
    media === 'cover'
      ? 'quick-look__art'
      : 'quick-look__art quick-look__art--wide';
  if (url) {
    return (
      <div className={shape}>
        <img
          src={url}
          alt={`${entry.name}, ${t(`art.media.${media}`)}`}
          draggable={false}
          className="quick-look__img"
        />
      </div>
    );
  }
  return (
    <div className={shape}>
      {/* The empty Slot (brand §2): a missing image is shown as missing. */}
      <div className="art-placeholder">
        {mark === 'present'
          ? ''
          : t(
              mark === 'missing' ? `art.none.${media}` : 'art.mark.unsupported',
            )}
      </div>
    </div>
  );
}

/**
 * Space in any view: the active entry's art, large (Finder's Quick Look).
 * ←/→ step through the visible entries; Space or Escape closes and focus goes
 * back to the view. A Radix modal, so the view shortcuts stand down while it
 * is open.
 */
export function QuickLook({
  entries,
  systems,
  generation,
  media,
  previewId,
  onSelect,
  onClose,
}: QuickLookProps) {
  const t = useT();
  const content = useRef<HTMLDivElement>(null);
  // There is no trigger: the view that opened it gets focus back.
  const opener = useRef<HTMLElement | null>(null);
  const index = useMemo(
    () =>
      previewId === null ? -1 : entries.findIndex((e) => e.id === previewId),
    [entries, previewId],
  );
  const entry = entries[index];
  const prev = entries[index - 1];
  const next = entries[index + 1];

  function onKeyDown(ev: KeyboardEvent<HTMLDivElement>) {
    if (ev.altKey || ev.ctrlKey || ev.metaKey) return;
    if (ev.key === ' ') {
      ev.preventDefault();
      onClose();
    } else if (ev.key === 'ArrowLeft' || ev.key === 'ArrowRight') {
      ev.preventDefault();
      const to = ev.key === 'ArrowLeft' ? prev : next;
      if (to) onSelect(to.id);
    }
  }

  const system = entry ? systems[entry.system] : undefined;
  return (
    <Dialog
      open={entry !== undefined}
      onOpenChange={(open) => !open && onClose()}
    >
      {entry && (
        <DialogContent
          ref={content}
          size="xl"
          className="quick-look"
          onKeyDown={onKeyDown}
          // The dialog itself takes focus, so the arrows step at once and
          // a screen reader reads the game's name first.
          onOpenAutoFocus={(ev) => {
            ev.preventDefault();
            opener.current =
              document.activeElement instanceof HTMLElement
                ? document.activeElement
                : null;
            content.current?.focus();
          }}
          onCloseAutoFocus={(ev) => {
            ev.preventDefault();
            if (opener.current?.isConnected) opener.current.focus();
            opener.current = null;
          }}
        >
          <DialogHeader>
            <DialogTitle className="quick-look__title">
              {entry.name}
            </DialogTitle>
            <DialogDescription asChild>
              <div className="quick-look__system">
                <SystemLabel
                  systemId={system?.system_id ?? null}
                  name={system?.name ?? ''}
                  size={16}
                />
              </div>
            </DialogDescription>
          </DialogHeader>
          <Art generation={generation} entry={entry} media={media} />
          <div className="quick-look__steps">
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label={t('layouts.previewPrev')}
              disabled={!prev}
              onClick={() => prev && onSelect(prev.id)}
            >
              <ChevronLeft aria-hidden />
            </Button>
            <span className="quick-look__position">
              {t('layouts.previewPosition', {
                n: index + 1,
                total: entries.length,
              })}
            </span>
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label={t('layouts.previewNext')}
              disabled={!next}
              onClick={() => next && onSelect(next.id)}
            >
              <ChevronRight aria-hidden />
            </Button>
          </div>
        </DialogContent>
      )}
    </Dialog>
  );
}
