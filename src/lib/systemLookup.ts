import type { SystemView } from '@/ipc/types';

/** The system a folder name or directory path belongs to, for surfaces that
 *  only carry a folder (fill summary, review queue). Exact `folder` first,
 *  then `dir`, then `folder` ignoring case. */
export function systemByFolder(
  systems: readonly SystemView[],
  folderOrDir: string,
): SystemView | undefined {
  const lower = folderOrDir.toLowerCase();
  return (
    systems.find((s) => s.folder === folderOrDir) ??
    systems.find((s) => s.dir === folderOrDir) ??
    systems.find((s) => s.folder.toLowerCase() === lower)
  );
}
