import type { EntryView, Media, SystemView } from '@/ipc/types';
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

/** What every library view takes (spec §B.4). `entries` is already in
 *  grouped order and `sections` are index ranges into it. */
export interface ViewProps extends EntryListProps {
  sections: readonly Section[];
  prefs: ViewPrefs;
  /** Space: quick look at the active entry. */
  onPreview(id: number): void;
  /** Mounts: Enter on a slot. */
  onOpenArt?(id: number, media: Media): void;
}

/** The entry id of the item an event happened in, if any. */
export function entryIdOf(target: EventTarget | null): number | null {
  const el =
    target instanceof Element ? target.closest('[data-entry-id]') : null;
  const raw = el?.getAttribute('data-entry-id');
  return raw == null ? null : Number(raw);
}
