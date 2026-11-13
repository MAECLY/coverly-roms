import type { EntryView, Media, SystemView } from '@/ipc/types';
import type { Game } from '@/lib/games';
import type { Section } from '@/lib/grouping';
import type { ViewPrefs } from '@/lib/viewPrefs';

export interface EntryListProps {
  entries: readonly EntryView[];
  systems: readonly SystemView[];
  generation: number;
  selectedId: number | null;
  /** The keyboard moved the selection. */
  onSelect(id: number): void;
  /** Click or Enter: select and show details. */
  onOpen(id: number): void;
}

/** A library's games, for the views that show their add-ons (part C). */
export interface ViewContent {
  /** Every revision's entry id → its game. */
  byEntry: ReadonlyMap<number, Game>;
  /** Any game of the library has add-on content: the tiles' summary line
   * (a library flag, so row heights never change with a filter). */
  line: boolean;
  /** Children's `rel_path`s tagged «Nuevo» (D18). */
  fresh: ReadonlySet<string>;
  /** A click on a game's summary: the panel, at its Versión section. */
  onOpenVersion?(id: number): void;
  /** A click on one of a game's updates or DLC (the Galería tray): the
   * panel, at that child. */
  onOpenChild?(id: number, childIndex: number): void;
}

/** What every library view takes (spec §B.4). `entries` is already in
 *  grouped order and `sections` are index ranges into it. */
export interface ViewProps extends EntryListProps {
  sections: readonly Section[];
  prefs: ViewPrefs;
  /** Space: quick look at the active entry. */
  onPreview(id: number): void;
  /** Mounts: Enter on a slot. */
  onOpenArt?(id: number, media: Media): void;
  /** The games behind `entries`; absent, the view shows entries only. */
  content?: ViewContent | undefined;
}

/** The entry id of the item an event happened in, if any. */
export function entryIdOf(target: EventTarget | null): number | null {
  const el =
    target instanceof Element ? target.closest('[data-entry-id]') : null;
  const raw = el?.getAttribute('data-entry-id');
  return raw == null ? null : Number(raw);
}
