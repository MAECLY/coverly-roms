const TEXT_INPUTS = new Set(['text', 'search', 'number', 'date']);

/** Text entry, where a key is a character and not a screen shortcut. */
export function isTyping(target: EventTarget | null): boolean {
  return (
    (target instanceof HTMLInputElement && TEXT_INPUTS.has(target.type)) ||
    target instanceof HTMLTextAreaElement ||
    target instanceof HTMLSelectElement
  );
}

/** Apple keyboards show ⌘ for the shortcut modifier; the rest, Ctrl. */
export function modKeyLabel(): string {
  const nav = typeof navigator === 'undefined' ? null : navigator;
  const platform = nav?.platform || nav?.userAgent || '';
  return /Mac|iPhone|iPad/i.test(platform) ? '⌘' : 'Ctrl';
}
