import { useEffect, useRef, useState } from 'react';

import { initialRate, pushSample, rateOf, type RateState } from '@/lib/rate';

/** Rate of `value` per second for `key` (null: idle), re-rendering at most
 * twice a second. Timestamps are taken here, on arrival (D4). */
export function useRate(
  key: number | string | null,
  value: number | null,
): number | null {
  const state = useRef<RateState>(initialRate());
  const [rate, setRate] = useState<number | null>(null);
  useEffect(() => {
    if (key === null || value === null) return;
    state.current = pushSample(state.current, key, value, performance.now());
  }, [key, value]);
  useEffect(() => {
    if (key === null) return;
    const id = window.setInterval(
      () => setRate(rateOf(state.current, performance.now())),
      500,
    );
    return () => window.clearInterval(id);
  }, [key]);
  return rate;
}
