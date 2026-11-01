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
  // Systems batch 1 (spec §9.2): no silhouette in the pinned set.
  msxturbor: 'TR',
  c128: 'C128',
  dragon: 'DRGN',
  coco: 'COCO',
  wasm4: 'W4',
  lowresnx: 'LRNX',
  vircon32: 'V32',
  wolf3d: 'W3D',
  solarus: 'SOL',
  model2: 'MDL2',
  model3: 'MDL3',
  naomi2: 'NAO2',
  triforce: 'TRI',
  // Systems batch 1: neutral set icon outside the 1:2 to 2.5:1 aspect range,
  // unreadable in a 1.5x row box.
  vectrex: 'VEC',
  cdtv: 'CDTV',
  amstradcpc: 'CPC',
  channelf: 'CHF',
  intellivision: 'INTV',
  // Systems batch 1: the set icon is a logo, wordmark or game art (D20).
  doom: 'DOOM',
  quake: 'QK',
  cavestory: 'CAVE',
  xrick: 'RICK',
  flashback: 'FLBK',
  mrboom: 'BOOM',
  dinothawr: 'DINO',
  lutro: 'LUTR',
  tic80: 'TIC',
  uzebox: 'UZE',
  // Systems batch 2: abbreviations until their silhouettes are generated
  // (plan Task 17).
  gamecube: 'GC',
  wii: 'WII',
  '3ds': '3DS',
  wiiu: 'WIIU',
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
  'doom',
  'quake',
  'cavestory',
  'xrick',
  'flashback',
  'mrboom',
  'dinothawr',
  'lutro',
  'tic80',
  'uzebox',
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
