import type { EntryView } from '@/ipc/types';

/** Lowercase without diacritics: `Pokémon` → `pokemon`. */
export function foldText(s: string): string {
  return s.normalize('NFD').replace(/\p{M}/gu, '').toLowerCase();
}

/** Search keys indexed by entry id (`name.ext`, folded). */
export function buildSearchKeys(entries: readonly EntryView[]): string[] {
  const keys: string[] = [];
  for (const e of entries) keys[e.id] = foldText(`${e.name}.${e.ext}`);
  return keys;
}

/** The folded words of a query; every word must appear in the key. */
export function queryTokens(query: string): string[] {
  return foldText(query).split(/\s+/).filter(Boolean);
}
