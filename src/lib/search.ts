import type { EntryView } from '@/ipc/types';

/** Lowercase without diacritics: `Pokémon` → `pokemon`. */
export function foldText(s: string): string {
  return s.normalize('NFD').replace(/\p{M}/gu, '').toLowerCase();
}

/**
 * Search keys indexed by entry id: `name.ext`, then the name of each of
 * its updates and DLC, folded. A game matches by its own name or any
 * child's name (addon views D1); names are joined by a newline, which no
 * query token holds, so a word never runs from one name into the next.
 */
export function buildSearchKeys(entries: readonly EntryView[]): string[] {
  const keys: string[] = [];
  for (const e of entries) {
    let key = `${e.name}.${e.ext}`;
    for (const c of e.children ?? []) key += `\n${c.name}`;
    keys[e.id] = foldText(key);
  }
  return keys;
}

/** The folded words of a query; every word must appear in the key. */
export function queryTokens(query: string): string[] {
  return foldText(query).split(/\s+/).filter(Boolean);
}
