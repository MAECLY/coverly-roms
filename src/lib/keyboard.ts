import type { RevealOs } from '@/lib/reveal';

const TEXT_INPUTS = new Set([
  'text',
  'search',
  'number',
  'date',
  // An API key field is a password field: a slash in it is a character.
  'password',
  'url',
  'email',
  'tel',
]);

/** Text entry, where a key is a character and not a screen shortcut. */
export function isTyping(target: EventTarget | null): boolean {
  return (
    (target instanceof HTMLInputElement && TEXT_INPUTS.has(target.type)) ||
    target instanceof HTMLTextAreaElement ||
    target instanceof HTMLSelectElement
  );
}

/** Apple platforms, where ⌘ is the shortcut modifier and Ctrl edits text. */
export function isMac(): boolean {
  const nav = typeof navigator === 'undefined' ? null : navigator;
  const platform = nav?.platform || nav?.userAgent || '';
  return /Mac|iPhone|iPad/i.test(platform);
}

/** Apple keyboards show ⌘ for the shortcut modifier; the rest, Ctrl. */
export function modKeyLabel(): string {
  return isMac() ? '⌘' : 'Ctrl';
}

/** The digit keys 1–9 (row or numpad) by physical key, so AZERTY's
 * unshifted «é» on the 2 key still reads 2. 0 is not a copy number. */
export function digitOf(ev: Pick<KeyboardEvent, 'code'>): number | null {
  const m = /^(?:Digit|Numpad)([1-9])$/.exec(ev.code ?? '');
  return m ? Number(m[1]) : null;
}

/** A letter shortcut: the typed key when it is a Latin letter, else the
 * physical key (`KeyJ` → `j`) for Cyrillic, Greek and similar layouts. */
export function letterOf(
  ev: Pick<KeyboardEvent, 'key' | 'code'>,
): string | null {
  if (/^[a-z]$/i.test(ev.key)) return ev.key.toLowerCase();
  const m = /^Key([A-Z])$/.exec(ev.code ?? '');
  return m ? m[1]!.toLowerCase() : null;
}

/** "Reveal in Finder / Explorer": ⌥⌘R on macOS, Shift+Alt+R elsewhere,
 * matched by `code` because ⌥R types «®». */
export function isRevealChord(
  ev: Pick<
    KeyboardEvent,
    'code' | 'metaKey' | 'altKey' | 'ctrlKey' | 'shiftKey'
  >,
  os: RevealOs,
): boolean {
  if (ev.code !== 'KeyR' || !ev.altKey || ev.ctrlKey) return false;
  return os === 'mac' ? ev.metaKey && !ev.shiftKey : ev.shiftKey && !ev.metaKey;
}
