import type { ThemeChoice } from '@/ipc/types';

export type ResolvedTheme = 'light' | 'dark';

export function resolveTheme(
  choice: ThemeChoice,
  prefersDark: boolean,
): ResolvedTheme {
  return choice === 'system' ? (prefersDark ? 'dark' : 'light') : choice;
}

/** Marks `<html>` (`data-theme`, `.dark`, `color-scheme`); for `system`,
 * follows the OS until the returned cleanup runs. */
export function applyTheme(
  choice: ThemeChoice,
  root: HTMLElement = document.documentElement,
): () => void {
  const mq =
    typeof window !== 'undefined' && typeof window.matchMedia === 'function'
      ? window.matchMedia('(prefers-color-scheme: dark)')
      : null;
  const set = () => {
    const resolved = resolveTheme(choice, mq?.matches ?? false);
    root.dataset.theme = resolved;
    root.classList.toggle('dark', resolved === 'dark');
    root.style.colorScheme = resolved;
  };
  set();
  if (choice !== 'system' || !mq) return () => undefined;
  mq.addEventListener('change', set);
  return () => mq.removeEventListener('change', set);
}
