/** D10 / AP-4: the pure sleeve queue behind `ExtractStage`. Kept apart
 * from the component so the module only exports components. */

/** At most this many sleeves in flight (D10); older ones land at once. */
export const FLY_CAP = 3;
/** AP-4: a sleeve whose `animationend` never arrives still lands. */
export const SAFETY_MS = 1500;

export interface Flight {
  /** Entry index (0-based) the sleeve stands for. */
  id: number;
  /** Stagger slot for `--g`. */
  g: number;
}
export interface StageState {
  item: number | null;
  /** Entries already accounted for: always `landed + flying.length`. */
  seen: number;
  landed: number;
  flying: Flight[];
}
export type StageAction =
  | {
      type: 'event';
      item: number;
      done: number;
      instant: boolean;
      cap?: number;
    }
  | { type: 'landed'; id: number }
  | { type: 'drain' };

export const initialStage: StageState = {
  item: null,
  seen: 0,
  landed: 0,
  flying: [],
};

/** D10: data first; at most `cap` sleeves in flight, the rest land at once.
 * The first event of an item is its baseline: what is already done stands
 * on the shelf at once (a stage mounted mid-item) and nothing replays. */
export function stageReducer(s: StageState, a: StageAction): StageState {
  switch (a.type) {
    case 'event': {
      if (a.item !== s.item)
        return { item: a.item, seen: a.done, landed: a.done, flying: [] };
      if (a.done <= s.seen) return s;
      const fresh = Array.from(
        { length: a.done - s.seen },
        (_, i) => s.seen + i,
      );
      if (a.instant)
        return {
          ...s,
          seen: a.done,
          landed: s.landed + s.flying.length + fresh.length,
          flying: [],
        };
      const room = Math.max(0, (a.cap ?? FLY_CAP) - s.flying.length);
      const fly = room > 0 ? fresh.slice(-room) : [];
      return {
        ...s,
        seen: a.done,
        landed: s.landed + fresh.length - fly.length,
        // Same array when nothing new flies, so the safety timer keeps running.
        flying:
          fly.length === 0
            ? s.flying
            : [...s.flying, ...fly.map((id, i) => ({ id, g: i }))],
      };
    }
    case 'landed':
      return s.flying.some((f) => f.id === a.id)
        ? {
            ...s,
            landed: s.landed + 1,
            flying: s.flying.filter((f) => f.id !== a.id),
          }
        : s;
    case 'drain':
      return s.flying.length === 0
        ? s
        : { ...s, landed: s.landed + s.flying.length, flying: [] };
  }
}
