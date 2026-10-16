const FOCUSABLE =
  'button:not([disabled]), textarea:not([disabled]), input:not([disabled]), select:not([disabled]), [href], [tabindex]:not([tabindex="-1"])';

/** Keeps Tab and Shift+Tab cycling inside `root`. */
export function trapTab(ev: KeyboardEvent, root: HTMLElement | null) {
  if (!root) return;
  const items = Array.from(root.querySelectorAll<HTMLElement>(FOCUSABLE));
  const first = items[0];
  const last = items[items.length - 1];
  const active = document.activeElement;
  if (!first || !last) {
    ev.preventDefault();
    root.focus();
  } else if (ev.shiftKey && (active === first || active === root)) {
    ev.preventDefault();
    last.focus();
  } else if (!ev.shiftKey && (active === last || !root.contains(active))) {
    ev.preventDefault();
    first.focus();
  }
}
