import { useSyncExternalStore } from 'react';

const QUERY = '(prefers-reduced-motion: reduce)';

function subscribe(cb: () => void): () => void {
  const mq = window.matchMedia?.(QUERY);
  if (!mq) return () => undefined;
  if (typeof mq.addEventListener === 'function') {
    mq.addEventListener('change', cb);
    return () => mq.removeEventListener('change', cb);
  }
  // WKWebView on macOS 11 (Safari 13) only has the deprecated pair.
  mq.addListener(cb);
  return () => mq.removeListener(cb);
}

const getSnapshot = (): boolean => window.matchMedia?.(QUERY).matches === true;

/** Whether the OS asks for reduced motion; follows changes live. */
export const useReducedMotion = (): boolean =>
  useSyncExternalStore(subscribe, getSnapshot, () => false);
