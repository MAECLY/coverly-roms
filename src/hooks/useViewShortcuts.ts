import { useEffect, useRef } from 'react';

import { stepSize } from '@/lib/coverSizes';
import type { CoverSize, ViewId, ViewPrefs } from '@/lib/viewPrefs';

/** Views with a cover size slider (spec §B.4.1). */
const SIZED_VIEWS: ReadonlySet<ViewId> = new Set(['covers', 'shelves']);
/** Accumulated wheel delta, in px, per size stop. */
export const WHEEL_STEP_PX = 100;
/** `WheelEvent.deltaMode` lines and pages, roughly in px. */
const LINE_PX = 16;
const PAGE_PX = 800;

export interface ViewShortcutOptions {
  prefs: ViewPrefs;
  setPrefs(patch: Partial<ViewPrefs>): void;
  /** False while something owns the keyboard (an overlay, the art workspace). */
  enabled?: boolean;
}

/** Text entry, where ⌘+ / ⌘− / ⌘0 may mean something to the field. */
function inField(target: EventTarget | null): boolean {
  return (
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    target instanceof HTMLSelectElement ||
    (target instanceof HTMLElement && target.isContentEditable)
  );
}

/** An open Radix dialog, sheet or alert: the library stays as it is behind it. */
function behindDialog(): boolean {
  return (
    document.querySelector(
      '[role="dialog"][data-state="open"], [role="alertdialog"][data-state="open"]',
    ) !== null
  );
}

function sizeForKey(key: string, size: CoverSize): CoverSize | null {
  if (key === '+' || key === '=') return stepSize(size, 1);
  if (key === '-' || key === '_') return stepSize(size, -1);
  if (key === '0') return 'm';
  return null;
}

/**
 * Library view shortcuts on `window`: ⌘+ (or ⌘=), ⌘− and ⌘0 move the cover
 * size one stop or back to M, and ⌘/Ctrl + wheel (a trackpad pinch arrives
 * as Ctrl + wheel) moves one stop per 100px of accumulated delta. Only in
 * Covers and Shelves; elsewhere the events are left alone. Handled ones are
 * `preventDefault`ed so the webview never zooms.
 */
export function useViewShortcuts({
  prefs,
  setPrefs,
  enabled = true,
}: ViewShortcutOptions): void {
  // Several stops can land between two renders (one wheel burst), so the
  // handlers step from the size they last asked for, not a stale render.
  const latest = useRef({ prefs, setPrefs, enabled });
  useEffect(() => {
    latest.current = { prefs, setPrefs, enabled };
  }, [prefs, setPrefs, enabled]);

  useEffect(() => {
    let wheelAcc = 0;

    function active(): boolean {
      const { prefs: p, enabled: on } = latest.current;
      return on && SIZED_VIEWS.has(p.view) && !behindDialog();
    }

    function resize(next: CoverSize): void {
      const cur = latest.current;
      if (next === cur.prefs.coverSize) return;
      cur.prefs = { ...cur.prefs, coverSize: next };
      cur.setPrefs({ coverSize: next });
    }

    function onKey(ev: KeyboardEvent): void {
      if (ev.defaultPrevented || ev.altKey || !(ev.metaKey || ev.ctrlKey)) {
        return;
      }
      if (inField(ev.target) || !active()) return;
      const next = sizeForKey(ev.key, latest.current.prefs.coverSize);
      if (next === null) return;
      ev.preventDefault();
      resize(next);
    }

    function onWheel(ev: WheelEvent): void {
      if (!(ev.ctrlKey || ev.metaKey) || !active()) return;
      ev.preventDefault();
      const scale =
        ev.deltaMode === 1 ? LINE_PX : ev.deltaMode === 2 ? PAGE_PX : 1;
      const delta = ev.deltaY * scale;
      // A turn the other way starts counting again.
      wheelAcc =
        Math.sign(delta) === Math.sign(wheelAcc) ? wheelAcc + delta : delta;
      while (Math.abs(wheelAcc) >= WHEEL_STEP_PX) {
        const dir = wheelAcc > 0 ? 1 : -1;
        wheelAcc -= dir * WHEEL_STEP_PX;
        // Towards the user (positive deltaY, pinch in) makes covers smaller.
        resize(stepSize(latest.current.prefs.coverSize, dir > 0 ? -1 : 1));
      }
    }

    window.addEventListener('keydown', onKey);
    window.addEventListener('wheel', onWheel, { passive: false });
    return () => {
      window.removeEventListener('keydown', onKey);
      window.removeEventListener('wheel', onWheel);
    };
  }, []);
}
