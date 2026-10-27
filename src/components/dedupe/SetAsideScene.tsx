import { useEffect, useRef, useState } from 'react';

import { useReducedMotion } from '@/hooks/useReducedMotion';

/** Lifts per second at most (spec D13): one every this many ms. */
const TICK_MS = 250;
/** A flight: 240 ms lift (`slow`) + 400 ms drop (`slower`). */
const FLIGHT_MS = 640;
/** The vacated slot stays dashed for a beat before the shelf closes up. */
const BEAT_MS = 400;
/** Above this many pending lifts the counter shows the truth at once. */
const MAX_QUEUE = 4;

/** Shelf geometry (viewBox 320×96): five 3:4 slots on a shelf rule. */
const SLOTS = 5;
const SLOT_W = 30;
const SLOT_H = 40;
const SLOT_STEP = 40;
const SLOT_X0 = 16;
const SHELF_Y = 80;
const LAST_X = SLOT_X0 + (SLOTS - 1) * SLOT_STEP;

/** The folder name fits the Manila tab; longer names end in «…». */
const TAB_CHARS = 12;
const tabLabel = (name: string) =>
  name.length > TAB_CHARS ? `${name.slice(0, TAB_CHARS - 1)}…` : name;

interface Flight {
  id: number;
}

/**
 * The set-aside while applying (spec D13): copies lift off a shelf of
 * slots and drop into a Manila-tabbed folder. Driven by real progress,
 * at most four lifts a second (bursts go in batches and the counter
 * jumps to the truth). Decoration only: the progress bar and its text
 * stay the accessible source, so the drawing is `aria-hidden`.
 */
export function SetAsideScene({
  done,
  total,
  stopped,
  folder,
}: {
  done: number;
  total: number;
  /** Cancel or pause: freeze where it is. */
  stopped: boolean;
  /** Shown on the Manila tab, e.g. `_Duplicados`. */
  folder: string;
}) {
  const reduce = useReducedMotion();
  const [count, setCount] = useState(done);
  const [flights, setFlights] = useState<Flight[]>([]);
  const [gap, setGap] = useState(false);
  const [shelf, setShelf] = useState(0);
  const [queued, setQueued] = useState(0);

  const seen = useRef(done);
  const frozen = useRef(false);
  const queue = useRef(0);
  const lastLift = useRef(Number.NEGATIVE_INFINITY);
  const nextId = useRef(0);
  const ticker = useRef<ReturnType<typeof setInterval> | null>(null);
  const timers = useRef(new Set<ReturnType<typeof setTimeout>>());

  const later = (ms: number, f: () => void) => {
    const id = setTimeout(() => {
      timers.current.delete(id);
      f();
    }, ms);
    timers.current.add(id);
  };
  const stopTicker = () => {
    if (ticker.current !== null) clearInterval(ticker.current);
    ticker.current = null;
  };
  const halt = () => {
    stopTicker();
    for (const id of timers.current) clearTimeout(id);
    timers.current.clear();
    // The queue state stays as it was: a stopped scene never closes.
    queue.current = 0;
  };

  // One lift: a batch of copies leaves the shelf as one slot.
  const lift = () => {
    const pending = queue.current;
    if (pending <= 0) return stopTicker();
    const batch = pending > MAX_QUEUE ? Math.ceil(pending / MAX_QUEUE) : 1;
    queue.current = pending - batch;
    setQueued(queue.current);
    lastLift.current = Date.now();
    setCount((c) => Math.max(c, seen.current - queue.current));
    const id = nextId.current++;
    setFlights((f) => [...f, { id }]);
    setGap(true);
    later(BEAT_MS, () => {
      setGap(false);
      setShelf((s) => s + 1);
    });
    later(FLIGHT_MS, () => setFlights((f) => f.filter((x) => x.id !== id)));
    if (queue.current === 0) stopTicker();
  };

  useEffect(() => {
    const delta = done - seen.current;
    seen.current = Math.max(seen.current, done);
    if (stopped) {
      frozen.current = true;
      return;
    }
    if (frozen.current) {
      // A stop that did not hold: drop what froze mid-air, show the truth.
      frozen.current = false;
      setFlights([]);
      setGap(false);
      setQueued(0);
      setCount(seen.current);
      return;
    }
    if (reduce) {
      setCount(seen.current);
      return;
    }
    if (delta <= 0) return;
    queue.current += delta;
    setQueued(queue.current);
    if (queue.current > MAX_QUEUE) setCount(seen.current);
    if (ticker.current === null) {
      if (Date.now() - lastLift.current >= TICK_MS) lift();
      if (queue.current > 0) ticker.current = setInterval(lift, TICK_MS);
    }
    // `lift` only touches refs and setters.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [done, stopped, reduce]);

  useEffect(() => {
    if (stopped) halt();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [stopped]);

  useEffect(() => {
    const live = timers.current;
    return () => {
      if (ticker.current !== null) clearInterval(ticker.current);
      for (const id of live) clearTimeout(id);
    };
  }, []);

  const closed = total > 0 && count >= total && queued === 0;

  return (
    <svg
      className="dup-scene"
      viewBox="0 0 320 96"
      aria-hidden="true"
      focusable="false"
      data-stopped={stopped ? 'true' : undefined}
      data-closed={closed ? 'true' : undefined}
      data-reduce={reduce ? 'true' : undefined}
    >
      {/* The shelf: slots close up from the left after each lift. */}
      <line
        className="dup-scene__rule"
        x1={8}
        x2={LAST_X + SLOT_W + 8}
        y1={SHELF_Y + 0.75}
        y2={SHELF_Y + 0.75}
      />
      <g key={shelf} className="dup-scene__shelf" data-closing={shelf > 0}>
        {Array.from({ length: SLOTS }, (_, i) => {
          const vacated = gap && i === SLOTS - 1;
          return (
            <rect
              key={i}
              className={
                vacated
                  ? 'dup-scene__slot dup-scene__slot--missing'
                  : i === 0
                    ? 'dup-scene__slot dup-scene__slot--fresh'
                    : 'dup-scene__slot'
              }
              x={SLOT_X0 + i * SLOT_STEP}
              y={SHELF_Y - SLOT_H}
              width={SLOT_W}
              height={SLOT_H}
              rx={2}
            />
          );
        })}
      </g>

      {/* The folder: back with its Manila tab, the flights, then the front. */}
      <g className="dup-scene__folder">
        <g className="dup-scene__tab">
          <path
            className="dup-scene__tab-paper"
            d="M232 40 V30 a3 3 0 0 1 3 -3 H297 a3 3 0 0 1 3 3 V40 Z"
          />
          <text className="dup-scene__name" x={238} y={37}>
            {tabLabel(folder)}
          </text>
        </g>
        <rect
          className="dup-scene__back"
          x={228}
          y={40}
          width={84}
          height={40}
          rx={3}
        />
      </g>

      {flights.map((f) => (
        <g
          key={f.id}
          className="dup-scene__flight"
          data-moving="true"
          data-flight={f.id}
        >
          <rect
            className="dup-scene__slot"
            x={LAST_X}
            y={SHELF_Y - SLOT_H}
            width={SLOT_W}
            height={SLOT_H}
            rx={2}
          />
        </g>
      ))}

      <rect
        className="dup-scene__front"
        x={228}
        y={46}
        width={84}
        height={34}
        rx={3}
      />
      <text
        className="dup-scene__count"
        x={270}
        y={69}
        textAnchor="middle"
        data-scene-count=""
      >
        {count}
      </text>
    </svg>
  );
}
