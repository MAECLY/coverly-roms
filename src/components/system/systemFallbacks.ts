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
  // Systems batch 1, disc family: the Slot mount until the RetroArch
  // silhouettes of spec §9.2 are generated (plan Task 15 removes these).
  ps2: 'PS2',
  '3do': '3DO',
  pcfx: 'PCFX',
  cdi: 'CDI',
  amigacd32: 'CD32',
  cdtv: 'CDTV',
  jaguarcd: 'JGCD',
  fmtowns: 'FMT',
  // Early consoles (plan Task 5), same interim Slot mount.
  colecovision: 'CV',
  intellivision: 'INTV',
  vectrex: 'VEC',
  odyssey2: 'O2',
  channelf: 'CHF',
  arcadia: 'ARC',
  scv: 'SCV',
  jaguar: 'JAG',
  // Sega, NEC and Nintendo add-ons (plan Task 6), same interim Slot mount.
  sg1000: 'SG1K',
  pico: 'PICO',
  supergrafx: 'SGX',
  n64dd: '64DD',
  satellaview: 'BSX',
  sufami: 'ST',
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

/**
 * Known abbreviation, else the first 4 letters of the id upper-cased (user systems).
 * Own-key lookup only: a user id such as `constructor` must not read Object.prototype.
 */
export function abbreviationFor(id: string): string {
  return (
    (Object.hasOwn(FALLBACK_ABBR, id) ? FALLBACK_ABBR[id] : undefined) ??
    (id
      .replace(/[^a-z0-9]/gi, '')
      .slice(0, 4)
      .toUpperCase() ||
      '?')
  );
}
