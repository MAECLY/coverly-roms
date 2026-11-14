/** Where a dropped item landed, in CSS pixels of the window. */
export interface DropPoint {
  x: number;
  y: number;
}

/** Marks the Extras section: an OS drop inside it adds extras (D11). */
export const EXTRAS_DROP_ROOT = 'data-extras-drop-root';

/** Open while a dialog, an alert or the media viewer shows. */
const MODAL = '[role="dialog"], [role="alertdialog"], [data-media-viewer]';

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
  if (document.querySelector(MODAL)) return true;
  return Array.from(document.querySelectorAll(`[${EXTRAS_DROP_ROOT}]`)).some(
    (el) => {
      const r = el.getBoundingClientRect();
      return (
        point.x >= r.left &&
        point.x <= r.right &&
        point.y >= r.top &&
        point.y <= r.bottom
      );
    },
  );
}
