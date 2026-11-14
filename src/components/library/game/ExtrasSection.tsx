import { Plus } from 'lucide-react';
import {
  useContext,
  useEffect,
  useMemo,
  useState,
  type KeyboardEvent,
} from 'react';

import { Alert } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { Spinner } from '@/components/ui/spinner';
import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import { errorKey, toBackendError, type BackendError } from '@/ipc/errors';
import {
  EXTRAS_CATEGORIES,
  type CategoryRef,
  type EntryView,
  type ExtraFileView,
  type ExtrasView,
  type Writability,
} from '@/ipc/types';
import {
  categoryKey,
  categoryLabel,
  sameCategory,
  sortCategories,
} from '@/lib/extras';
import { useExtrasStore } from '@/stores/extrasStore';
import { currentTrack, usePlayerStore } from '@/stores/playerStore';

import { ExtrasCategory, VIRTUALISE_AFTER } from './ExtrasCategory';
import { ExtrasMissing } from './ExtrasMissing';

const REASON: Record<Exclude<Writability['kind'], 'yes'>, string> = {
  'read-only': 'extras.readOnly',
  busy: 'extras.busy',
  newer: 'extras.newer',
};

const NAV_KEYS = new Set([
  'ArrowDown',
  'ArrowUp',
  'ArrowLeft',
  'ArrowRight',
  'Home',
  'End',
]);

interface Band {
  category: CategoryRef;
  files: ExtraFileView[];
}

/** The bands with rows (present categories, user ones even when empty,
 * loose files when any) and the fixed categories left as dashed slots. */
function bandsOf(listing: ExtrasView): {
  bands: Band[];
  slots: CategoryRef[];
} {
  const bands: Band[] = [];
  const slots: CategoryRef[] = [];
  for (const c of listing.categories) {
    const files = listing.files.filter((f) =>
      sameCategory(f.category, c.category),
    );
    // A user category is a folder the user made: shown even when empty.
    if (c.category.kind === 'user' || files.length > 0)
      bands.push({ category: c.category, files });
    else if (c.category.kind === 'fixed') slots.push(c.category);
  }
  // A fixed category the listing did not name is a slot too.
  for (const name of EXTRAS_CATEGORIES)
    if (
      !listing.categories.some(
        (c) => c.category.kind === 'fixed' && c.category.name === name,
      )
    )
      slots.push({ kind: 'fixed', name });
  return {
    bands: sortCategories(bands),
    slots: sortCategories(slots.map((category) => ({ category }))).map(
      (s) => s.category,
    ),
  };
}

/**
 * «Extras» in the game panel (extras spec §8.1, §8.2, §8.4): the game
 * folder's `Extras/` as category bands of file rows, the empty suggested
 * categories as one line of dashed slots, «No encontrados» last. Opening
 * it never writes (D3); the reason writes are off, if any, is one line,
 * and play and view stay available. One roving tab stop across the rows.
 */
export function ExtrasSection({
  generation,
  entry,
  onAdd,
  onOpen,
  onPlay,
}: {
  generation: number;
  entry: EntryView;
  /** «Añadir archivos» or a category's «+» (the add flow). */
  onAdd?: ((category?: CategoryRef) => void) | undefined;
  onOpen?: ((index: number) => void) | undefined;
  onPlay?: ((index: number) => void) | undefined;
}) {
  const t = useT();
  const locale = useContext(LocaleContext);
  const open = useExtrasStore((s) => s.open);
  const stored = useExtrasStore((s) =>
    s.generation === generation && s.listing?.entry_id === entry.id
      ? s.listing
      : null,
  );
  const loading = useExtrasStore((s) => s.loading);
  const error = useExtrasStore((s) => s.error);
  const track = usePlayerStore(currentTrack);
  const [writeError, setWriteError] = useState<BackendError | null>(null);
  const [focused, setFocused] = useState<number | null>(null);
  // Bands the user opened or closed, by category key; the rest follow the
  // default (a band over 200 files starts closed, §8.2).
  const [toggled, setToggled] = useState<Record<string, boolean>>({});

  useEffect(() => {
    void open(generation, entry.id);
  }, [open, generation, entry.id]);

  const listing = stored;
  const layout = useMemo(() => (listing ? bandsOf(listing) : null), [listing]);

  if (listing && !listing.game_folder) return null;

  const canWrite = listing?.writable.kind === 'yes';
  const reason =
    listing && listing.writable.kind !== 'yes'
      ? REASON[listing.writable.kind]
      : null;
  const isOpen = (b: Band) =>
    toggled[categoryKey(b.category)] ?? b.files.length <= VIRTUALISE_AFTER;
  // The tab stop goes only to a row on screen: never into a closed band.
  const files = layout
    ? layout.bands.filter(isOpen).flatMap((b) => b.files)
    : [];
  const tabStop =
    focused !== null && files.some((f) => f.index === focused)
      ? focused
      : (files[0]?.index ?? null);
  const playingPath =
    track && track.generation === generation && track.entryId === entry.id
      ? track.path
      : null;
  const empty =
    listing !== null &&
    listing.files.length === 0 &&
    layout !== null &&
    layout.bands.length === 0;
  const manualsBand = layout?.bands.some(
    (b) => b.category.kind === 'fixed' && b.category.name === 'manuals',
  );
  const num = new Intl.NumberFormat(locale);

  function onKeyDown(e: KeyboardEvent<HTMLElement>) {
    if (e.defaultPrevented || !NAV_KEYS.has(e.key)) return;
    if (e.altKey || e.metaKey || e.ctrlKey || e.shiftKey) return;
    const target = e.target as HTMLElement;
    const row = target.closest<HTMLElement>('[data-extras-row]');
    if (!row || !target.hasAttribute('data-extras-ctl')) return;
    let next: HTMLElement | undefined;
    if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
      const ctls = [...row.querySelectorAll<HTMLElement>('[data-extras-ctl]')];
      const at = ctls.indexOf(target);
      next = ctls[at + (e.key === 'ArrowRight' ? 1 : -1)];
    } else {
      const primaries = [
        ...e.currentTarget.querySelectorAll<HTMLElement>(
          '[data-extras-primary]',
        ),
      ];
      const at = primaries.indexOf(
        row.querySelector<HTMLElement>('[data-extras-primary]')!,
      );
      next =
        e.key === 'Home'
          ? primaries[0]
          : e.key === 'End'
            ? primaries.at(-1)
            : primaries[at + (e.key === 'ArrowDown' ? 1 : -1)];
    }
    // The keys stay in the section: the library behind it never sees them.
    e.preventDefault();
    e.stopPropagation();
    next?.focus();
  }

  async function forget(index: number) {
    setWriteError(null);
    try {
      await useExtrasStore.getState().forget(index);
    } catch (err) {
      setWriteError(toBackendError(err));
    }
  }

  const shownError = writeError ?? error;

  return (
    // eslint-disable-next-line jsx-a11y/no-noninteractive-element-interactions -- arrow keys bubble from the rows' buttons (roving focus, §8.4)
    <section
      id="panel-extras"
      data-panel-section="extras"
      data-entry={listing?.entry_id}
      className="panel-sec panel-extras"
      aria-labelledby="panel-extras-h"
      aria-busy={loading || undefined}
      onKeyDown={onKeyDown}
    >
      <div className="extras-head">
        <h3 id="panel-extras-h" className="panel-sec__h extras-head__title">
          {t('extras.title')}
          {listing && (
            <small className="mono">{num.format(listing.files.length)}</small>
          )}
        </h3>
        {loading && <Spinner size="sm" className="extras-head__spin" />}
        <Button
          variant="ghost"
          size="sm"
          className="extras-head__add"
          disabled={!canWrite}
          onClick={() => onAdd?.()}
        >
          <Plus aria-hidden />
          {t('extras.add')}
        </Button>
      </div>
      {reason && <p className="panel-note extras-reason">{t(reason)}</p>}
      {listing?.index.state === 'malformed' && (
        <Alert variant="warning" className="extras-alert">
          {t('extras.malformed')}
        </Alert>
      )}
      {shownError && (
        <Alert variant="danger" className="extras-alert">
          {t(errorKey(shownError.code), { detail: shownError.detail ?? '' })}
        </Alert>
      )}
      {empty && <p className="extras-empty">{t('extras.empty')}</p>}
      {layout?.bands.map((b) => (
        <ExtrasCategory
          key={categoryKey(b.category)}
          category={b.category}
          files={b.files}
          canWrite={canWrite}
          tabStop={tabStop}
          playingPath={playingPath}
          manualNote={
            listing!.manual_inside &&
            b.category.kind === 'fixed' &&
            b.category.name === 'manuals'
          }
          open={isOpen(b)}
          onToggle={() =>
            setToggled((o) => ({
              ...o,
              [categoryKey(b.category)]: !isOpen(b),
            }))
          }
          onFocusRow={setFocused}
          onAdd={onAdd}
          onOpen={onOpen}
          onPlay={onPlay}
        />
      ))}
      {layout && layout.slots.length > 0 && (
        <ul className="extras-slots">
          {layout.slots.map((c) => {
            const label = categoryLabel(c, t);
            return (
              <li key={categoryKey(c)}>
                <button
                  type="button"
                  className="extras-slot"
                  aria-label={t('extras.addTo', { category: label })}
                  disabled={!canWrite}
                  onClick={() => onAdd?.(c)}
                >
                  {label}
                </button>
              </li>
            );
          })}
        </ul>
      )}
      {listing?.manual_inside && !manualsBand && (
        <p className="panel-note">{t('extras.manualInside')}</p>
      )}
      {listing && listing.missing.length > 0 && (
        <ExtrasMissing
          missing={listing.missing}
          canWrite={canWrite}
          onRescan={() => void useExtrasStore.getState().refresh()}
          onForget={(i) => void forget(i)}
        />
      )}
    </section>
  );
}
