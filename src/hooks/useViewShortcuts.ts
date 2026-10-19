import { useEffect, useRef } from 'react';

import { stepSize } from '@/lib/coverSizes';
import {
  INSPECTOR_VIEWS,
  VIEW_IDS,
  type CoverSize,
  type ViewId,
  type ViewPrefs,
} from '@/lib/viewPrefs';

/** Views with a cover size slider (spec §B.4.1). */
const SIZED_VIEWS: ReadonlySet<ViewId> = new Set(['covers', 'shelves']);
const INSPECTOR_SET: ReadonlySet<ViewId> = new Set(INSPECTOR_VIEWS);
const ALL_VIEWS: ReadonlySet<ViewId> = new Set(VIEW_IDS);
/** Accumulated wheel delta, in px, per size stop. */
export const WHEEL_STEP_PX = 100;
/** `WheelEvent.deltaMode` lines and pages, roughly in px. */
const LINE_PX = 16;
const PAGE_PX = 800;
/** A pause this long (ms) ends a wheel gesture: leftover delta is dropped. */
const WHEEL_IDLE_MS = 400;

export interface ViewShortcutOptions {
  prefs: ViewPrefs;
  setPrefs(patch: Partial<ViewPrefs>): void;
  /** False while something owns the keyboard (an overlay, the art workspace). */
  enabled?: boolean;
}

/** Text entry, where ⌘+ / ⌘− / ⌘0 / ⌘I may mean something to the field. */
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

/**
 * The view ⌘1–⌘6 asks for, by the digit typed or, where the digit row
 * types something else without Shift (AZERTY), by the physical key.
 */
function viewForKey(ev: KeyboardEvent): ViewId | null {
  const digit = /^[1-9]$/.test(ev.key)
    ? ev.key
    : /^Digit[1-9]$/.test(ev.code)
      ? ev.code.slice(-1)
      : null;
  return digit === null ? null : (VIEW_IDS[Number(digit) - 1] ?? null);
}

function sizeForKey(key: string, size: CoverSize): CoverSize | null {
  if (key === '+' || key === '=') return stepSize(size, 1);
  if (key === '-' || key === '_') return stepSize(size, -1);
  if (key === '0') return 'm';
  return null;
}

/**
 * Library view shortcuts on `window`: ⌘1–⌘6 (or Ctrl) switch views in
 * switcher order, also from the search box. ⌘+ (or ⌘=), ⌘− and ⌘0 move the cover
 * size one stop or back to M, and ⌘/Ctrl + wheel (a trackpad pinch arrives
 * as Ctrl + wheel) moves one stop per 100px of accumulated delta. Only in
 * Covers and Shelves; elsewhere the events are left alone. ⌘I docks or
 * undocks the inspector, except in Columns and Gallery. Handled ones are
 * `preventDefault`ed so the webview never zooms (nor shows page info).
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
    let wheelAt = 0;

    function active(views: ReadonlySet<ViewId>): boolean {
      const { prefs: p, enabled: on } = latest.current;
      return on && views.has(p.view) && !behindDialog();
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
      // ⌘1–⌘6 never type text, so they also work from the search box.
      const view = ev.shiftKey ? null : viewForKey(ev);
      if (view !== null) {
        if (!active(ALL_VIEWS)) return;
        ev.preventDefault();
        const cur = latest.current;
        if (view === cur.prefs.view) return;
        cur.prefs = { ...cur.prefs, view };
        cur.setPrefs({ view });
        return;
      }
      if (inField(ev.target)) return;
      if (ev.key === 'i' || ev.key === 'I') {
        // ⌘⇧I / Ctrl+Shift+I is the webview's Web Inspector: leave it alone.
        // 'I' alone still counts, for Caps Lock.
        if (ev.shiftKey || !active(INSPECTOR_SET)) return;
        ev.preventDefault();
        const cur = latest.current;
        const inspector = !cur.prefs.inspector;
        cur.prefs = { ...cur.prefs, inspector };
        cur.setPrefs({ inspector });
        return;
      }
      if (!active(SIZED_VIEWS)) return;
      const next = sizeForKey(ev.key, latest.current.prefs.coverSize);
      if (next === null) return;
      ev.preventDefault();
      resize(next);
    }

    function onWheel(ev: WheelEvent): void {
      if (!(ev.ctrlKey || ev.metaKey) || !active(SIZED_VIEWS)) return;
      ev.preventDefault();
      const scale =
        ev.deltaMode === 1 ? LINE_PX : ev.deltaMode === 2 ? PAGE_PX : 1;
      const delta = ev.deltaY * scale;
      // A turn the other way, or a new gesture, starts counting again.
      const fresh = ev.timeStamp - wheelAt > WHEEL_IDLE_MS;
      wheelAt = ev.timeStamp;
      wheelAcc =
        !fresh && Math.sign(delta) === Math.sign(wheelAcc)
          ? wheelAcc + delta
          : delta;
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
