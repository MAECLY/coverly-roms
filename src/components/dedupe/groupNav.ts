/**
 * J / K across the duplicate groups (spec D12). One `onKeyDown` on the
 * list that holds the system sections, never a window listener: a single
 * letter that fires anywhere fails WCAG 2.1.4 and clashes with
 * screen-reader quick-nav keys.
 *
 * Each open section registers its groups (ids in display order and a way
 * to scroll its window to one), so J/K walk the data, including groups the
 * window has not mounted yet. A closed section unmounts its rows and its
 * registration, so it is skipped; so is anything outside the sections
 * (the "Sin cambios" list behind «Mostrar más»).
 */
import {
  createContext,
  useContext,
  useLayoutEffect,
  type RefObject,
} from 'react';

export interface GroupList {
  /** Group ids in display order. */
  ids: readonly number[];
  /** Mount the group at this index and bring it into view. */
  scrollToIndex(index: number): void;
}

export type GroupRegistry = Map<Element, GroupList>;

export const GroupNavContext = createContext<GroupRegistry | null>(null);

/** Registers a section's list for J/K while it is mounted. */
export function useRegisterGroupList(
  ref: RefObject<HTMLElement | null>,
  list: GroupList,
) {
  const registry = useContext(GroupNavContext);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!registry || !el) return;
    registry.set(el, list);
    return () => {
      registry.delete(el);
    };
    // Every render: the list element changes when the window turns on.
  });
}

/** A dialog on top owns the keyboard. */
export function dialogOpen(): boolean {
  return (
    document.querySelector(
      '[role="dialog"][data-state="open"], [role="alertdialog"][data-state="open"]',
    ) !== null
  );
}

/** Focus a group: its keeper radio, else the group itself. */
function focusGroup(el: HTMLElement) {
  const target =
    el.querySelector<HTMLElement>('[role="radio"][data-state="checked"]') ??
    el.querySelector<HTMLElement>('[role="radio"]') ??
    el;
  target.focus({ preventScroll: true });
  el.scrollIntoView?.({ block: 'nearest' });
}

interface Stop {
  list: Element;
  entry: GroupList;
  index: number;
  id: number;
}

const groupSel = (id: number) => `[data-dup-group="${id}"]`;

/** Moves focus one group forwards or back; `false` when there is none. */
export function stepGroup(
  container: HTMLElement,
  registry: GroupRegistry,
  step: 1 | -1,
): boolean {
  const stops: Stop[] = [];
  for (const list of container.querySelectorAll('.dup-system__groups')) {
    const entry = registry.get(list);
    if (!entry) continue;
    entry.ids.forEach((id, index) => stops.push({ list, entry, index, id }));
  }
  if (stops.length === 0) return false;

  const active = document.activeElement;
  const here = active?.closest<HTMLElement>('[data-dup-group]');
  let at = -1;
  if (here) {
    const id = Number(here.dataset.dupGroup);
    at = stops.findIndex((s) => s.id === id && s.list.contains(here));
  }
  let next: Stop | undefined;
  if (at >= 0) next = stops[at + step];
  else if (active) {
    // Focus outside any group (a section head): the first group after it,
    // or the last one before it.
    const after = (s: Stop) =>
      Boolean(
        active.compareDocumentPosition(s.list) &
        Node.DOCUMENT_POSITION_FOLLOWING,
      ) || s.list.contains(active);
    next =
      step === 1
        ? stops.find(after)
        : [...stops].reverse().find((s) => !after(s));
  }
  if (!next) return false;

  const mounted = next.list.querySelector<HTMLElement>(groupSel(next.id));
  if (mounted) {
    focusGroup(mounted);
    return true;
  }
  const target = next;
  target.entry.scrollToIndex(target.index);
  // The window mounts it on the next render; focus it then.
  let tries = 0;
  const later = () => {
    const el = target.list.querySelector<HTMLElement>(groupSel(target.id));
    if (el) focusGroup(el);
    else if (++tries < 5) requestAnimationFrame(later);
  };
  requestAnimationFrame(later);
  return true;
}
