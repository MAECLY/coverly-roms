import { useCallback, useState } from 'react';

import { useT } from '@/i18n/useT';

/** Says where an item moved after an ↑ / ↓ button, for screen readers. */
// eslint-disable-next-line react-refresh/only-export-components -- the hook and its region go together
export function useReorderAnnouncer() {
  const t = useT();
  const [message, setMessage] = useState('');
  const moved = useCallback(
    (name: string, position: number, total: number) =>
      setMessage(t('settings.moved', { name, position, total })),
    [t],
  );
  return { message, moved };
}

/** A polite live region (no `status` role: panels keep one real status). */
export function ReorderAnnouncer({ message }: { message: string }) {
  return (
    <div aria-live="polite" aria-atomic="true" className="sr-only">
      {message}
    </div>
  );
}
