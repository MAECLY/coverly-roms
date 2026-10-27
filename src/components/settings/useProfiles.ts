import { useEffect, useState } from 'react';

import { getBackend } from '@/ipc/backend';
import type { ProfileInfo } from '@/ipc/types';

/** The layout profiles, loaded once per panel: Library, Organise and
 * Duplicates name profiles by id. Empty until the list loads, or if it
 * cannot. */
export function useProfiles(): ProfileInfo[] {
  const [profiles, setProfiles] = useState<ProfileInfo[]>([]);
  useEffect(() => {
    let alive = true;
    void getBackend()
      .then((b) => b.listProfiles())
      .then(
        (p) => alive && setProfiles(p),
        () => undefined,
      );
    return () => {
      alive = false;
    };
  }, []);
  return profiles;
}
