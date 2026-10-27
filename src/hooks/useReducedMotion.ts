import { useEffect, useState } from 'react';

const REDUCE_MOTION = '(prefers-reduced-motion: reduce)';

/** Whether the system asks for reduced motion, following later changes. */
export function useReducedMotion(): boolean {
  const [reduce, setReduce] = useState(
    () => window.matchMedia?.(REDUCE_MOTION).matches === true,
  );
  useEffect(() => {
    const mq = window.matchMedia?.(REDUCE_MOTION);
    if (!mq) return;
    const on = () => setReduce(mq.matches);
    mq.addEventListener?.('change', on);
    return () => mq.removeEventListener?.('change', on);
  }, []);
  return reduce;
}
