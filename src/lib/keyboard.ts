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
