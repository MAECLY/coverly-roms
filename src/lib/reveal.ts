/**
 * The per-OS wording and chord of "show in Finder / Explorer / the file
 * manager", shared by the library drawer and the duplicates screen.
 */
export type RevealOs = 'mac' | 'win' | 'other';

export function revealOs(
  ua = typeof navigator === 'undefined' ? '' : navigator.userAgent,
): RevealOs {
  if (/Mac/i.test(ua)) return 'mac';
  if (/Windows/i.test(ua)) return 'win';
  return 'other';
}

const LABEL = {
  mac: 'drawer.revealMac',
  win: 'drawer.revealWin',
  other: 'drawer.revealOther',
} as const;

/** «Mostrar en Finder» / «Mostrar en el Explorador» / «Mostrar en la carpeta». */
export const revealLabelKey = (os = revealOs()) => LABEL[os];

/** Whole-sentence accessible names (spec §8): `dedupe.revealCopy.{os}` / `dedupe.revealFolder.{os}`. */
export const revealCopyKey = (os = revealOs()) =>
  `dedupe.revealCopy.${os}` as const;
export const revealFolderKey = (os = revealOs()) =>
  `dedupe.revealFolder.${os}` as const;

/** The chord shown and advertised: ⌥⌘R on macOS, Shift+Alt+R elsewhere. */
export const revealShortcut = (os = revealOs()) =>
  os === 'mac'
    ? { label: '⌥⌘R', aria: 'Meta+Alt+R' }
    : { label: 'Shift+Alt+R', aria: 'Shift+Alt+R' };
