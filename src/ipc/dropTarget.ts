/** Where a dropped item landed, in CSS pixels of the window. */
export interface DropPoint {
  x: number;
  y: number;
}

/** Marks the Extras section: an OS drop inside it adds extras (D11). */
export const EXTRAS_DROP_ROOT = 'data-extras-drop-root';

/** Open while a dialog, an alert or the media viewer shows. */
const MODAL = '[role="dialog"], [role="alertdialog"], [data-media-viewer]';

/** Whether a dialog, an alert or the media viewer is open: a drop then
 * neither opens a library nor adds extras (D11). */
export function modalOpen(): boolean {
  return document.querySelector(MODAL) !== null;
}

/** Whether `point` falls inside `el`'s box. */
export function pointIn(el: Element, point: DropPoint): boolean {
  const r = el.getBoundingClientRect();
  return (
    point.x >= r.left &&
    point.x <= r.right &&
    point.y >= r.top &&
    point.y <= r.bottom
  );
}

/** The Extras category (band or dashed slot, `data-extras-band`) under
 * `point` inside `root`, or null. */
export function bandAt(root: Element, point: DropPoint): string | null {
  for (const el of root.querySelectorAll<HTMLElement>('[data-extras-band]'))
    if (pointIn(el, point)) return el.dataset.extrasBand ?? null;
  return null;
}

/**
 * The point of a drop the OS reported in physical pixels: the webview
 * hit-tests in CSS pixels, so it is divided by `devicePixelRatio` (D11).
 */
export function toCssPoint(physical: DropPoint): DropPoint {
  const ratio = window.devicePixelRatio || 1;
  return { x: physical.x / ratio, y: physical.y / ratio };
}

/**
 * Whether a drop at `point` is not a library to open (part D D11): it
 * falls inside the Extras section, which adds the files itself, or lands
 * while a dialog or the viewer is open.
 */
export function dropIsNotALibrary(point: DropPoint): boolean {
  if (modalOpen()) return true;
  return Array.from(document.querySelectorAll(`[${EXTRAS_DROP_ROOT}]`)).some(
    (el) => pointIn(el, point),
  );
}
