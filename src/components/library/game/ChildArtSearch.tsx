import { Search } from 'lucide-react';
import { useEffect, useId, useRef, useState, type KeyboardEvent } from 'react';

import { CandidateImage } from '@/components/fill/CandidateImage';
import { CandidateMeta } from '@/components/fill/CandidateMeta';
import { Alert } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Spinner } from '@/components/ui/spinner';
import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import { errorKey, toBackendError, type BackendError } from '@/ipc/errors';
import type { CandidateView, ChildView, EntryView } from '@/ipc/types';

/**
 * «Buscar arte» for one update or DLC (addon views D11, §14 Q3): the
 * drawer's title search, aimed at the child. It looks the query up once
 * when it opens and on each submit; Rust lists only ScreenScraper and
 * SteamGridDB results (the origins a child slot records), and nothing is
 * written until the user picks one: never on its own, because DLC names
 * match poorly. The child is named by its index, never by a path.
 */
export function ChildArtSearch({
  generation,
  entry,
  child,
  initialQuery,
  onWritten,
  onClose,
}: {
  generation: number;
  /** The game's entry: its search and its children's handle. */
  entry: EntryView;
  child: ChildView;
  initialQuery: string;
  onWritten(children: ChildView[]): void;
  onClose(): void;
}) {
  const t = useT();
  const uid = useId();
  const [query, setQuery] = useState(initialQuery);
  const [asked, setAsked] = useState<{ seq: number; q: string }>({
    seq: 1,
    q: initialQuery,
  });
  const [found, setFound] = useState<{
    seq: number;
    items: CandidateView[];
  } | null>(null);
  const [writing, setWriting] = useState(false);
  const [error, setError] = useState<BackendError | null>(null);
  const boxRef = useRef<HTMLInputElement>(null);
  /** Escape in the box or on a result closes the search (not the panel). */
  const onEscape = (e: KeyboardEvent) => {
    if (e.key !== 'Escape') return;
    e.preventDefault();
    e.stopPropagation();
    onClose();
  };

  useEffect(() => {
    boxRef.current?.focus();
  }, []);

  useEffect(() => {
    let live = true;
    void (async () => {
      try {
        const b = await getBackend();
        const items = await b.searchChildArt(
          generation,
          entry.id,
          child.index,
          asked.q,
        );
        if (live)
          setFound({
            seq: asked.seq,
            items,
          });
      } catch (e) {
        if (live) {
          setError(toBackendError(e));
          setFound({ seq: asked.seq, items: [] });
        }
      }
    })();
    return () => {
      live = false;
    };
  }, [asked, generation, entry.id, child.index]);

  const loading = found?.seq !== asked.seq;

  async function pick(c: CandidateView) {
    if (writing) return;
    setError(null);
    setWriting(true);
    try {
      const b = await getBackend();
      const children = await b.setChildArtFromSearch(
        generation,
        entry.id,
        child.index,
        'icon',
        c.index,
      );
      onWritten(children);
    } catch (e) {
      setError(toBackendError(e));
    } finally {
      setWriting(false);
    }
  }

  return (
    <div className="panel-search" role="group" aria-labelledby={`${uid}-h`}>
      <div className="panel-search__head">
        <p id={`${uid}-h`} className="panel-search__title">
          {t('panel.searchArtFor', { name: child.name })}
        </p>
        <Button variant="ghost" size="sm" onClick={onClose}>
          {t('panel.searchArtClose')}
        </Button>
      </div>
      <form
        className="flex gap-1.5"
        role="search"
        onSubmit={(e) => {
          e.preventDefault();
          const q = query.trim();
          // A pick names a result of the search on screen: a new search
          // while it is written could make it write one of the new list.
          if (!q || writing) return;
          setError(null);
          setAsked((a) => ({ seq: a.seq + 1, q }));
        }}
      >
        <label className="relative flex min-w-0 flex-1 items-center">
          <span className="sr-only">{t('drawer.searchLabel')}</span>
          <Search
            aria-hidden
            className="pointer-events-none absolute left-2 size-3.5 text-cr-text-secondary"
          />
          <Input
            ref={boxRef}
            type="search"
            name="child-art-search"
            size="sm"
            className="pl-7"
            value={query}
            maxLength={120}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={onEscape}
          />
        </label>
        <Button type="submit" size="sm" disabled={!query.trim() || writing}>
          {t('drawer.searchGo')}
        </Button>
      </form>
      <p className="panel-note">{t('panel.searchArtOnly')}</p>
      {error && (
        <Alert variant="danger" className="mt-2">
          {t(errorKey(error.code), { detail: error.detail ?? '' })}
        </Alert>
      )}
      {loading ? (
        <p role="status" className="panel-note flex items-center gap-2">
          <Spinner size="sm" decorative />
          {t('drawer.loadingCandidates')}
        </p>
      ) : found && found.items.length === 0 ? (
        !error && <p className="panel-note">{t('panel.searchArtNone')}</p>
      ) : (
        <div
          role="group"
          aria-label={t('drawer.searchResults')}
          className="panel-search__grid"
        >
          {found?.items.map((c) => (
            <button
              key={c.index}
              type="button"
              className="candidate panel-search__pick"
              disabled={writing}
              onKeyDown={onEscape}
              onClick={() => void pick(c)}
            >
              <span className="panel-search__pic">
                <CandidateImage
                  source={{
                    kind: 'child-search',
                    generation,
                    entryId: entry.id,
                    childIndex: child.index,
                    candidate: c.index,
                  }}
                  label={c.name}
                />
              </span>
              <span className="panel-search__name" title={c.name}>
                {c.name}
              </span>
              <CandidateMeta c={c} position={c.index + 1} compact />
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
