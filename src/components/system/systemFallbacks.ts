/** Abbreviations for systems with no neutral silhouette (spec §A.4). ≤ 4 chars. */
export const FALLBACK_ABBR: Readonly<Record<string, string>> = {
  cps1: 'CPS1',
  cps2: 'CPS2',
  cps3: 'CPS3',
  mame: 'MAME',
  hbmame: 'HB',
  dos: 'DOS',
  scummvm: 'SVM',
  easyrpg: 'RPG',
  ons: 'ONS',
  openbor: 'BOR',
  pico8: 'P8',
  atomiswave: 'AW',
  naomi: 'NAO',
};

/** Ids whose set icon is a wordmark/logo: never mapped to a silhouette. */
export const WORDMARK_IDS: readonly string[] = [
  'cps1',
  'cps2',
  'cps3',
  'mame',
  'hbmame',
  'dos',
  'scummvm',
  'easyrpg',
];

/** Known abbreviation, else the first 4 letters of the id upper-cased (user systems). */
export function abbreviationFor(id: string): string {
  return (
    FALLBACK_ABBR[id] ??
    (id
      .replace(/[^a-z0-9]/gi, '')
      .slice(0, 4)
      .toUpperCase() ||
      '?')
  );
}
