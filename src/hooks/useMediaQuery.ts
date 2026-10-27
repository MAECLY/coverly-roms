import { useCallback, useSyncExternalStore } from 'react';

/** Whether a media query matches now; follows its `change` events. */
export function useMediaQuery(query: string): boolean {
  const subscribe = useCallback(
    (notify: () => void) => {
      const list = window.matchMedia?.(query);
      list?.addEventListener?.('change', notify);
      return () => list?.removeEventListener?.('change', notify);
    },
    [query],
  );
  return useSyncExternalStore(
    subscribe,
    () => window.matchMedia?.(query).matches ?? false,
    () => false,
  );
}
