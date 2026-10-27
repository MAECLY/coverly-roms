import { useEffect, useRef, useState } from 'react';

import {
  initialRate,
  MIN_SPAN_MS,
  pushSample,
  rateOf,
  type RateState,
} from '@/lib/rate';

/** Rate of `value` per second for `key` (null: idle), re-rendering at most
 * twice a second. Timestamps are taken here, on arrival (D4). The last rate
 * carries across keys of one run; going idle forgets it, so the next run
 * starts measuring instead of showing the previous one's rate.
 * `minSpanMs` (default 2 s) is how long a run must last before a rate shows. */
export function useRate(
  key: number | string | null,
  value: number | null,
  { minSpanMs = MIN_SPAN_MS }: { minSpanMs?: number } = {},
): number | null {
  const state = useRef<RateState>(initialRate());
  const [rate, setRate] = useState<number | null>(null);
  useEffect(() => {
    if (key === null || value === null) return;
    state.current = pushSample(state.current, key, value, performance.now());
  }, [key, value]);
  const idle = key === null;
  const [wasIdle, setWasIdle] = useState(idle);
  if (idle !== wasIdle) {
    setWasIdle(idle);
    if (idle) setRate(null);
  }
  useEffect(() => {
    if (idle) state.current = initialRate();
  }, [idle]);
  useEffect(() => {
    if (key === null) return;
    const id = window.setInterval(
      () => setRate(rateOf(state.current, performance.now(), minSpanMs)),
      500,
    );
    return () => window.clearInterval(id);
  }, [key, minSpanMs]);
  return idle ? null : rate;
}
