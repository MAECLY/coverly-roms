import * as DialogPrimitive from '@radix-ui/react-dialog';
import {
  ChevronLeft,
  ChevronRight,
  ExternalLink,
  FolderOpen,
  Maximize,
  X,
  ZoomIn,
  ZoomOut,
} from 'lucide-react';
import {
  useContext,
  useEffect,
  useRef,
  useState,
  type KeyboardEvent,
} from 'react';

import { Button } from '@/components/ui/button';
import {
  CenterLayer,
  DialogDescription,
  DialogOverlay,
  DialogPortal,
  DialogTitle,
} from '@/components/ui/dialog';
import { useRichT } from '@/i18n/rich';
import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import type { ExtrasTextView, ExtrasView } from '@/ipc/types';
import { categoryLabel, extOf } from '@/lib/extras';
import { revealLabelKey } from '@/lib/reveal';
import { useReducedMotion } from '@/lib/useReducedMotion';
import { cn } from '@/lib/utils';
import { routeOf, viewerQueue, zoomStep } from '@/lib/viewer';
import { usePlayerStore } from '@/stores/playerStore';
import { useViewerStore } from '@/stores/viewerStore';

import { ImageView } from './ImageView';
import { PdfView, type PdfHandle, type PdfStep } from './PdfView';
import { TextView } from './TextView';
import { UnplayableView } from './UnplayableView';
import { VideoView } from './VideoView';

type Content =
  | { state: 'loading' }
  | { state: 'media'; url: string }
  | { state: 'text'; text: ExtrasTextView }
  | { state: 'unplayable' };

/** The keys the viewer owns: never the library's while it is open (D23). */
const ZOOM_KEYS: Record<string, 1 | -1 | 0> = {
  '+': 1,
  '=': 1,
  '-': -1,
  _: -1,
  '0': 0,
};

/** The PDF page keys (D18, §8.3). */
const PAGE_KEYS: Record<string, PdfStep> = {
  PageDown: 'next',
  PageUp: 'previous',
  Home: 'first',
  End: 'last',
};

/** Whether `el` has content past its sides to scroll to. */
function scrollsSideways(el: HTMLElement | null): boolean {
  return el !== null && el.scrollWidth > el.clientWidth;
}

/**
 * The Extras viewer (extras D17, D18, D19, D23, §8.1): video, images,
 * PDF, text and an honest «doesn't play here» for the rest, in a Dialog
 * at up to 90 vw × 90 vh on the sunken surface. The header names the file
 * (Archivo, the one accent), its category and its place in mono; the
 * footer steps through the category (←/→), zooms images and PDFs
 * (+ − 0), counts PDF pages (PageUp/PageDown/Home/End) and reveals the
 * file or hands an allowed type to another app. It fetches
 * by the listing's `(rev, index)` handle, never a path; a video pauses
 * the mini player; the focus goes back to the file's row on close.
 * Opened through `viewerStore`, which also stands the library's window
 * shortcuts down.
 */
export function MediaViewer({
  generation,
  listing,
}: {
  generation: number;
  listing: ExtrasView | null;
}) {
  const t = useT();
  const tr = useRichT();
  const locale = useContext(LocaleContext);
  const reduced = useReducedMotion();
  const target = useViewerStore((s) => s.open);
  const entryId = listing?.entry_id ?? null;
  const ours =
    target !== null &&
    listing !== null &&
    target.generation === generation &&
    target.entryId === entryId;
  const file = ours
    ? (listing.files.find((f) => f.path === target.path) ?? null)
    : null;
  const queue = file && listing ? viewerQueue(listing, file) : [];
  const at = file ? queue.findIndex((f) => f.index === file.index) : -1;
  const route = file ? routeOf(file) : null;

  // What was fetched, for the file it names (game and path).
  const path = file?.path ?? null;
  const key = path ? `${generation}:${String(entryId)}:${path}` : null;
  const [content, setContent] = useState<{
    key: string;
    content: Content;
  } | null>(null);
  // Zoom belongs to one file: another file starts fitted (reset while
  // rendering, not in an effect).
  const [zoomed, setZoomed] = useState<{ key: string; zoom: number } | null>(
    null,
  );
  const [fitted, setFitted] = useState<{ key: string; fit: number } | null>(
    null,
  );
  const zoom = zoomed && zoomed.key === key ? zoomed.zoom : null;
  const fit = fitted && fitted.key === key ? fitted.fit : 1;
  const fetches =
    route === 'image' ||
    route === 'video' ||
    route === 'pdf' ||
    route === 'text';
  const pdfRef = useRef<PdfHandle>(null);
  // The PDF's page counter, for the file it belongs to.
  const [paged, setPaged] = useState<{
    key: string;
    page: number;
    pages: number;
  } | null>(null);
  const pageAt = paged && paged.key === key ? paged : null;
  const shown: Content = !fetches
    ? { state: 'unplayable' }
    : content && content.key === key
      ? content.content
      : { state: 'loading' };
  // The row the focus goes back to; kept past the close.
  const returnTo = useRef<number | null>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const latest = useRef(listing);
  useEffect(() => {
    latest.current = listing;
    if (file) returnTo.current = file.index;
  });

  // Its file left the listing (trashed, moved out): nothing to show.
  useEffect(() => {
    if (ours && listing && !file) useViewerStore.getState().close();
  }, [ours, listing, file]);

  // The section unmounts (another game, the panel closes): so does this.
  useEffect(
    () => () => {
      const open = useViewerStore.getState().open;
      if (open && open.generation === generation && open.entryId === entryId)
        useViewerStore.getState().close();
    },
    [generation, entryId],
  );

  // Only one thing plays at a time (D19).
  useEffect(() => {
    if (key && route === 'video') usePlayerStore.getState().pauseForVideo();
  }, [key, route]);

  // Fetch the shown file by the newest handle of its path; a refusal gives
  // up on it. A new `rev` of the same file keeps what is shown.
  useEffect(() => {
    if (!key || !fetches) return;
    const now = latest.current;
    const f = now?.files.find((x) => x.path === path);
    if (!now || !f) return;
    let live = true;
    const settle = (c: Content) => {
      if (live) setContent({ key, content: c });
    };
    const handle = [generation, now.entry_id, now.rev, f.index] as const;
    getBackend()
      .then((b) =>
        route === 'text'
          ? b
              .extrasReadText(...handle)
              .then((text): Content => ({ state: 'text', text }))
          : b
              .extrasMedia(...handle)
              .then((m): Content => ({ state: 'media', url: m.url })),
      )
      .then(settle, () => settle({ state: 'unplayable' }));
    return () => {
      live = false;
    };
  }, [key, path, fetches, route, generation]);

  function unplayable() {
    if (key) setContent({ key, content: { state: 'unplayable' } });
  }

  function go(dir: 1 | -1) {
    const next = queue[at + dir];
    if (next && target)
      useViewerStore.getState().show({ ...target, path: next.path });
  }

  async function handOver(how: 'reveal' | 'external') {
    if (!file || !listing) return;
    const b = await getBackend();
    const args = [
      generation,
      listing.entry_id,
      listing.rev,
      file.index,
    ] as const;
    try {
      if (how === 'reveal') await b.extrasReveal(...args);
      else await b.extrasOpenExternal(...args);
    } catch {
      // Gone or refused: the next listing says why.
    }
  }

  const image = route === 'image' && shown.state === 'media';
  const pdf = route === 'pdf' && shown.state === 'media';
  const zooms = image || pdf;
  function zoomBy(dir: 1 | -1 | 0) {
    if (!key) return;
    setZoomed(dir === 0 ? null : { key, zoom: zoomStep(zoom, dir, fit) });
  }

  function onKeyDown(e: KeyboardEvent<HTMLElement>) {
    // A modal: no key reaches the panel or the library behind it.
    e.stopPropagation();
    if (e.key === 'Escape') return;
    const el = e.target;
    // The video's own controls use the arrows (seek) and Space.
    const media = el instanceof HTMLMediaElement;
    const inField =
      el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement;
    if (e.altKey || inField) return;
    const own = () => {
      e.preventDefault();
      e.stopPropagation();
    };
    // A stage that overflows sideways (a zoomed image, a wide .nfo) keeps
    // ←/→ for panning when the focus is inside it.
    const pans =
      el instanceof Element &&
      scrollsSideways(el.closest<HTMLElement>('.media-viewer__stage'));
    if (!e.metaKey && !e.ctrlKey && !media && !pans) {
      const dir = e.key === 'ArrowLeft' ? -1 : e.key === 'ArrowRight' ? 1 : 0;
      if (dir !== 0) {
        own();
        go(dir);
        return;
      }
    }
    const z = ZOOM_KEYS[e.key];
    if (z !== undefined) {
      // Always ours while open, so ⌘+ never zooms the webview or the covers.
      own();
      if (zooms) zoomBy(z);
      return;
    }
    const step = PAGE_KEYS[e.key];
    if (step && pdf && !media && !e.metaKey && !e.ctrlKey) {
      own();
      pdfRef.current?.step(step);
    }
  }

  const open = file !== null;
  const num = new Intl.NumberFormat(locale);
  const canReveal = file !== null;
  const canExternal =
    file !== null &&
    file.playable !== 'reveal-only' &&
    shown.state !== 'unplayable';

  return (
    <DialogPrimitive.Root
      open={open}
      onOpenChange={(o) => {
        if (!o) useViewerStore.getState().close();
      }}
    >
      {file && listing && (
        <DialogPortal>
          <DialogOverlay
            className={cn(
              'data-[state=open]:duration-fast',
              reduced && 'animate-none data-[state=open]:animate-none',
            )}
          />
          <CenterLayer>
            <DialogPrimitive.Content
              className={cn(
                'media-viewer pointer-events-auto',
                !reduced && 'media-viewer--fade',
              )}
              data-route={route}
              onKeyDown={onKeyDown}
              ref={contentRef}
              onOpenAutoFocus={(e) => {
                // The viewer itself takes the keys; no button pre-focused.
                e.preventDefault();
                contentRef.current?.focus();
              }}
              onCloseAutoFocus={(e) => {
                const row = document.querySelector<HTMLElement>(
                  `[data-file="${String(returnTo.current)}"] [data-extras-primary]`,
                );
                if (!row) return;
                e.preventDefault();
                row.focus();
              }}
              tabIndex={-1}
            >
              <header className="media-viewer__head">
                <DialogTitle
                  className="media-viewer__title font-display"
                  title={file.name}
                >
                  {file.name}
                </DialogTitle>
                <DialogDescription asChild>
                  <p className="media-viewer__meta">
                    <span>{categoryLabel(file.category, t)}</span>
                    {queue.length > 1 && (
                      <span>
                        {tr(
                          'viewer.of',
                          {
                            i: num.format(at + 1),
                            n: num.format(queue.length),
                          },
                          ['i', 'n'],
                        )}
                      </span>
                    )}
                  </p>
                </DialogDescription>
                <DialogPrimitive.Close asChild>
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    className="media-viewer__close text-cr-text-secondary"
                    aria-label={t('ui.close')}
                  >
                    <X aria-hidden />
                  </Button>
                </DialogPrimitive.Close>
              </header>

              <div
                className="media-viewer__body"
                aria-busy={shown.state === 'loading'}
              >
                {shown.state === 'unplayable' && (
                  <UnplayableView
                    name={file.name}
                    playable={file.playable}
                    onReveal={() => void handOver('reveal')}
                    onOpenExternal={() => void handOver('external')}
                  />
                )}
                {shown.state === 'media' && route === 'image' && (
                  <ImageView
                    key={file.path}
                    url={shown.url}
                    name={file.name}
                    zoom={zoom}
                    onFit={(f) => key && setFitted({ key, fit: f })}
                    onUnplayable={unplayable}
                  />
                )}
                {shown.state === 'media' && route === 'pdf' && (
                  <PdfView
                    key={file.path}
                    ref={pdfRef}
                    url={shown.url}
                    name={file.name}
                    zoom={zoom}
                    onFit={(f) => key && setFitted({ key, fit: f })}
                    onPage={(page, pages) =>
                      key && setPaged({ key, page, pages })
                    }
                    onUnplayable={unplayable}
                    canExternal={file.playable !== 'reveal-only'}
                    onOpenExternal={() => void handOver('external')}
                  />
                )}
                {shown.state === 'media' && route === 'video' && (
                  <VideoView
                    key={file.path}
                    url={shown.url}
                    name={file.name}
                    onUnplayable={unplayable}
                  />
                )}
                {shown.state === 'text' && (
                  <TextView
                    view={shown.text}
                    mono={extOf(file.name) === 'nfo'}
                  />
                )}
              </div>

              <footer className="media-viewer__bar">
                <div className="media-viewer__group">
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    aria-label={t('viewer.previous')}
                    disabled={at <= 0}
                    onClick={() => go(-1)}
                  >
                    <ChevronLeft aria-hidden />
                  </Button>
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    aria-label={t('viewer.next')}
                    disabled={at < 0 || at + 1 >= queue.length}
                    onClick={() => go(1)}
                  >
                    <ChevronRight aria-hidden />
                  </Button>
                </div>
                {pdf && pageAt && (
                  <span className="media-viewer__page" aria-live="polite">
                    {tr(
                      'viewer.page',
                      {
                        i: num.format(pageAt.page),
                        n: num.format(pageAt.pages),
                      },
                      ['i', 'n'],
                    )}
                  </span>
                )}
                {zooms && (
                  <div className="media-viewer__group">
                    <Button
                      variant="ghost"
                      size="icon-sm"
                      aria-label={t('viewer.zoomOut')}
                      onClick={() => zoomBy(-1)}
                    >
                      <ZoomOut aria-hidden />
                    </Button>
                    <span className="media-viewer__zoom" aria-live="polite">
                      {tr(
                        'viewer.zoomLevel',
                        { n: num.format(Math.round((zoom ?? fit) * 100)) },
                        ['n'],
                      )}
                    </span>
                    <Button
                      variant="ghost"
                      size="icon-sm"
                      aria-label={t('viewer.zoomIn')}
                      onClick={() => zoomBy(1)}
                    >
                      <ZoomIn aria-hidden />
                    </Button>
                    <Button
                      variant="ghost"
                      size="icon-sm"
                      aria-label={t('viewer.fit')}
                      aria-pressed={zoom === null}
                      onClick={() => zoomBy(0)}
                    >
                      <Maximize aria-hidden />
                    </Button>
                  </div>
                )}
                {shown.state !== 'unplayable' && (
                  <div className="media-viewer__group media-viewer__group--end">
                    {canExternal && (
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => void handOver('external')}
                      >
                        <ExternalLink aria-hidden />
                        {t('viewer.openExternal')}
                      </Button>
                    )}
                    {canReveal && (
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => void handOver('reveal')}
                      >
                        <FolderOpen aria-hidden />
                        {t(revealLabelKey())}
                      </Button>
                    )}
                  </div>
                )}
              </footer>
            </DialogPrimitive.Content>
          </CenterLayer>
        </DialogPortal>
      )}
    </DialogPrimitive.Root>
  );
}
