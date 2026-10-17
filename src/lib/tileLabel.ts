/**
 * What a cover tile shows under the art: the game's name without the No-Intro
 * region group (the regions get their own short tags), and those short tags.
 */
import type { Region } from '@/ipc/types';

/** Region words as No-Intro / Redump spell them inside the region group. */
const REGION_WORDS = new Set([
  'world',
  'usa',
  'europe',
  'japan',
  'asia',
  'australia',
  'brazil',
  'canada',
  'china',
  'france',
  'germany',
  'hong kong',
  'italy',
  'korea',
  'netherlands',
  'spain',
  'sweden',
  'taiwan',
  'uk',
  'united kingdom',
]);

/** Short, language-neutral tags; `world` is translated by the caller. */
export const REGION_CODE: Record<Exclude<Region, 'world'>, string> = {
  usa: 'US',
  europe: 'EU',
  japan: 'JP',
  asia: 'AS',
  australia: 'AU',
  brazil: 'BR',
  canada: 'CA',
  china: 'CN',
  france: 'FR',
  germany: 'DE',
  'hong-kong': 'HK',
  italy: 'IT',
  korea: 'KR',
  netherlands: 'NL',
  spain: 'ES',
  sweden: 'SE',
  taiwan: 'TW',
  'united-kingdom': 'UK',
};

/**
 * `name` without its region group: "Golden Sun (USA, Europe) (Rev 1)" reads
 * "Golden Sun (Rev 1)". Other tags stay; a name that is only a region group
 * is returned whole.
 */
export function titleWithoutRegions(name: string): string {
  const stripped = name
    .replace(/\s*\(([^()]*)\)/, (group, inner: string) => {
      const words = inner.split(',').map((w) => w.trim().toLowerCase());
      return words.length > 0 && words.every((w) => REGION_WORDS.has(w))
        ? ''
        : group;
    })
    .trim();
  return stripped === '' ? name : stripped;
}
