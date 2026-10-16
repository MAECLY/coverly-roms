import { useEffect, useState } from 'react';

import type { AppInfo } from '@/ipc/appInfo';
import { getBackend } from '@/ipc/backend';

/** The app and core versions (null until the backend answers, or if it fails). */
export function useAppInfo(): AppInfo | null {
  const [info, setInfo] = useState<AppInfo | null>(null);
  useEffect(() => {
    let alive = true;
    void getBackend()
      .then((b) => b.appInfo())
      .then(
        (i) => alive && setInfo(i),
        () => undefined,
      );
    return () => {
      alive = false;
    };
  }, []);
  return info;
}
