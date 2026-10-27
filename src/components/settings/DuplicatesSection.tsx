import { DEDUPE_LEVELS } from '@/ipc/types';

import { DedupeAdvanced, DedupeCommon } from './DedupeSettings';
import { AdvancedFold } from './Field';
import { useProfiles } from './useProfiles';

/** Settings entries inside the fold: each level, the keeper rules, the
 * format order and the systems left out. */
const ADVANCED_COUNT = DEDUPE_LEVELS.length + 3;

/** Settings paths whose errors live inside the fold (it opens on them). */
const ADVANCED_PATHS = [
  'dedupe.levels',
  'dedupe.keep_rules',
  'dedupe.container_order',
  'dedupe.exclude_systems',
] as const;

/**
 * Settings › Duplicados (spec §5.3): the defaults of the Duplicates screen
 * on top; what the plan itself reads, folded under «Opciones avanzadas».
 */
export function DuplicatesSection() {
  const profiles = useProfiles();
  return (
    <div className="settings-group">
      <DedupeCommon />
      <AdvancedFold
        id="duplicates-advanced"
        count={ADVANCED_COUNT}
        errorPaths={ADVANCED_PATHS}
      >
        <DedupeAdvanced profiles={profiles} />
      </AdvancedFold>
    </div>
  );
}
