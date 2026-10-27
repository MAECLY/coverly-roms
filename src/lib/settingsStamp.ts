import type { Settings } from '@/ipc/types';

/**
 * What a duplicates re-plan reads from the settings (settings UI spec
 * §8.6). The action, levels, one-ROM switch and folder are only what the
 * screen starts with: the screen sets them itself, so they are left out.
 */
export const dedupeStamp = (s: Settings): string =>
  JSON.stringify([
    s.dedupe.keep_rules,
    s.dedupe.container_order,
    s.dedupe.exclude_systems,
    s.dedupe.quarantine_name,
    s.matching.regions,
  ]);

/** What an organise re-scan falls back to (settings UI spec §8.6). */
export const organizeStamp = (s: Settings): string =>
  JSON.stringify([s.organize, s.dedupe.organize_step]);
