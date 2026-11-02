/**
 * The numbers of the size questions before an apply (systems batch 3 D23,
 * adjustments D15–D17), worked out only from what the backend's
 * `large-transfer` refusal carried: the UI never sums plan sizes itself.
 */
import type { HugeItem, LargeTransferDetail, OrganizePlan } from '@/ipc/types';

/** Bytes from which Organizar asks before writing (Rust `LARGE_TRANSFER`). */
export const LARGE_TRANSFER = 10_000_000_000;

/** What writing `sum` more bytes leaves free on a target with `free`
 * bytes, with the apply's 1 % margin (Rust `check_space`, CLI
 * `room_after`): the bytes left, or the bytes missing. */
export function roomAfter(
  free: number,
  sum: number,
): { fits: true; left: number } | { fits: false; missing: number } {
  const needed = sum + Math.ceil(sum / 100);
  return free >= needed
    ? { fits: true, left: free - needed }
    : { fits: false, missing: needed - free };
}

/** The bytes the accepted huge items write, followers included. */
export function acceptedBytes(
  detail: LargeTransferDetail,
  accepted: readonly number[],
): number {
  return detail.huge
    .filter((h) => accepted.includes(h.item))
    .reduce((n, h) => n + h.bytes, 0);
}

/** Room left on the target once `item` is written after the rest and
 * the items accepted so far; null when the free space is unknown. */
export function roomForItem(
  detail: LargeTransferDetail,
  accepted: readonly number[],
  item: HugeItem,
): ReturnType<typeof roomAfter> | null {
  if (detail.free === null) return null;
  return roomAfter(
    detail.free,
    detail.rest + acceptedBytes(detail, accepted) + item.bytes,
  );
}

/** The aggregate question for what the non-huge items write (D16b):
 * the same dialog, with `rest` and the items left once every huge item
 * and its followers are taken out. */
export function restDetail(detail: LargeTransferDetail): LargeTransferDetail {
  if (detail.huge.length === 0) return detail;
  const taken = detail.huge.reduce((n, h) => n + 1 + h.followers.length, 0);
  return {
    ...detail,
    bytes: detail.rest,
    items: Math.max(0, detail.items - taken),
    huge: [],
  };
}

/** Whether the aggregate question follows the huge items (D16b). */
export function asksRest(detail: LargeTransferDetail): boolean {
  return detail.huge.length > 0 && detail.rest >= LARGE_TRANSFER;
}

/** Whether an apply with `declined` left would still do anything: an
 * item placed in the library or a duplicate set aside, other than the
 * declined items and the overlays that follow them. */
export function stillMoves(
  plan: OrganizePlan,
  detail: LargeTransferDetail,
  declined: readonly number[],
): boolean {
  const out = new Set(declined);
  for (const h of detail.huge)
    if (out.has(h.item)) h.followers.forEach((f) => out.add(f));
  return plan.items.some(
    (i) =>
      !out.has(i.id) &&
      ((i.action === 'auto' && i.target !== null) || i.action === 'set-aside'),
  );
}
