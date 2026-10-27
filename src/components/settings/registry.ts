import { DEDUPE_LEVELS } from '@/ipc/types';
import type { SettingsSection } from '@/stores/navStore';

export type SettingsFold =
  'library-advanced' | 'organize-advanced' | 'duplicates-advanced';

export interface SettingEntry {
  /** DOM id to focus: settingId(path) of the control, or a list anchor (SP-1). */
  id: string;
  section: SettingsSection;
  /** The AdvancedFold id that holds it, if any. */
  fold?: SettingsFold;
  /** i18n keys. */
  label: string;
  hint?: string;
  keywords?: string;
  /** The id is a prefix: dynamic rows (container order per profile/kind). */
  family?: true;
}

/**
 * Every setting a search can land on (spec §5.6), in tab order. The
 * completeness test (`registry.test.tsx`) fails when a `setting-` control in
 * any tab has no entry here, or an entry has no control.
 */
export const SETTINGS_INDEX: readonly SettingEntry[] = [
  // General
  {
    id: 'setting-general-language',
    section: 'general',
    label: 'settings.general.language',
  },
  {
    id: 'setting-general-theme',
    section: 'general',
    label: 'settings.general.theme',
  },
  {
    id: 'setting-general-check-updates-on-launch',
    section: 'general',
    label: 'settings.general.checkUpdates',
  },
  {
    id: 'setting-general-update-channel',
    section: 'general',
    label: 'settings.general.channel',
    hint: 'update.channelHint',
  },
  // Biblioteca
  {
    id: 'setting-library-default-profile',
    section: 'library',
    label: 'settings.library.defaultProfile',
  },
  {
    id: 'setting-library-include-hidden',
    section: 'library',
    label: 'settings.library.includeHidden',
    keywords: 'settings.keywords.hidden',
  },
  {
    id: 'setting-library-follow-symlinks',
    section: 'library',
    label: 'settings.library.followSymlinks',
    hint: 'settings.library.followSymlinksHint',
    keywords: 'settings.keywords.symlinks',
  },
  {
    id: 'setting-library-ignored-dirs',
    section: 'library',
    fold: 'library-advanced',
    label: 'settings.library.ignoredDirs',
    hint: 'settings.library.ignoredDirsHint',
  },
  {
    id: 'setting-library-ignored-exts',
    section: 'library',
    fold: 'library-advanced',
    label: 'settings.library.ignoredExts',
    hint: 'settings.library.ignoredExtsHint',
  },
  // Organizar
  {
    id: 'setting-organize-mode',
    section: 'organize',
    label: 'settings.organize.mode',
  },
  {
    id: 'setting-organize-default-layout',
    section: 'organize',
    label: 'settings.organize.defaultLayout',
  },
  {
    id: 'setting-organize-names',
    section: 'organize',
    label: 'settings.organize.names',
  },
  {
    id: 'setting-organize-nest-in-roms-root',
    section: 'organize',
    label: 'settings.organize.nestInRoms',
  },
  {
    id: 'setting-organize-download-dats',
    section: 'organize',
    label: 'settings.organize.downloadDats',
  },
  {
    id: 'setting-dedupe-organize-step',
    section: 'organize',
    label: 'settings.dedupe.organizeStep',
    keywords: 'settings.keywords.duplicates',
  },
  {
    id: 'setting-organize-external-tools',
    section: 'organize',
    fold: 'organize-advanced',
    label: 'settings.organize.externalTools',
    keywords: 'settings.keywords.rar',
  },
  // Duplicados
  {
    id: 'setting-dedupe-action-quarantine',
    section: 'duplicates',
    label: 'settings.dedupe.actionQuarantine',
    keywords: 'settings.keywords.quarantine',
  },
  {
    id: 'setting-dedupe-action-trash',
    section: 'duplicates',
    label: 'settings.dedupe.actionTrash',
    hint: 'settings.dedupe.actionTrashHint',
    keywords: 'settings.keywords.trash',
  },
  {
    id: 'setting-dedupe-quarantine-dir',
    section: 'duplicates',
    label: 'settings.dedupe.quarantine',
    keywords: 'settings.keywords.quarantine',
  },
  {
    id: 'setting-dedupe-one-game-one-rom',
    section: 'duplicates',
    label: 'settings.dedupe.oneGameOneRom',
    hint: 'settings.dedupe.oneGameOneRomHint',
  },
  ...DEDUPE_LEVELS.map((level): SettingEntry => ({
    id: `setting-dedupe-levels-${level}`,
    section: 'duplicates',
    fold: 'duplicates-advanced',
    label: `dedupe.level.${level}`,
  })),
  {
    id: 'setting-dedupe-keep-rules',
    section: 'duplicates',
    fold: 'duplicates-advanced',
    label: 'settings.dedupe.rules',
    keywords: 'settings.keywords.region',
  },
  {
    id: 'setting-dedupe-container-order',
    section: 'duplicates',
    fold: 'duplicates-advanced',
    label: 'settings.dedupe.containerOrder',
    family: true,
  },
  {
    id: 'setting-dedupe-exclude-systems',
    section: 'duplicates',
    fold: 'duplicates-advanced',
    label: 'settings.dedupe.exclude',
    hint: 'settings.dedupe.excludeHint',
  },
  // Arte
  {
    id: 'setting-art-media-cover',
    section: 'art',
    label: 'art.media.cover',
    hint: 'settings.art.media',
  },
  {
    id: 'setting-art-media-screenshot',
    section: 'art',
    label: 'art.media.screenshot',
    hint: 'settings.art.media',
  },
  {
    id: 'setting-art-media-title',
    section: 'art',
    label: 'art.media.title',
    hint: 'settings.art.media',
  },
  {
    id: 'setting-art-single-slot-media',
    section: 'art',
    label: 'settings.art.singleSlot',
    hint: 'settings.art.singleSlotHint',
  },
  {
    id: 'setting-art-output-format',
    section: 'art',
    label: 'settings.art.format',
    hint: 'settings.art.formatHint',
  },
  {
    id: 'setting-art-keep-aspect',
    section: 'art',
    label: 'settings.art.keepAspect',
  },
  {
    id: 'setting-art-jpeg-quality',
    section: 'art',
    label: 'settings.art.jpegQuality',
  },
  {
    id: 'setting-art-overwrite-never',
    section: 'art',
    label: 'settings.art.overwrites.never',
    hint: 'settings.art.overwrite',
  },
  {
    id: 'setting-art-overwrite-ask',
    section: 'art',
    label: 'settings.art.overwrites.ask',
    hint: 'settings.art.overwrite',
  },
  {
    id: 'setting-art-overwrite-backup',
    section: 'art',
    label: 'settings.art.overwrites.backup',
    hint: 'settings.art.overwrite',
  },
  // Coincidencias
  {
    id: 'setting-matching-strategy-rom-region',
    section: 'matching',
    label: 'fill.strategy.rom-region',
    hint: 'fill.strategyHint.rom-region',
  },
  {
    id: 'setting-matching-strategy-priority',
    section: 'matching',
    label: 'fill.strategy.priority',
    hint: 'fill.strategyHint.priority',
  },
  {
    id: 'setting-matching-strategy-popularity',
    section: 'matching',
    label: 'fill.strategy.popularity',
    hint: 'fill.strategyHint.popularity',
  },
  {
    id: 'setting-matching-regions',
    section: 'matching',
    label: 'settings.matching.regions',
    keywords: 'settings.keywords.region',
  },
  {
    id: 'setting-matching-languages',
    section: 'matching',
    label: 'settings.matching.languages',
    hint: 'settings.matching.languagesHint',
  },
  {
    id: 'setting-matching-auto-threshold',
    section: 'matching',
    label: 'settings.matching.auto',
  },
  {
    id: 'setting-matching-review-threshold',
    section: 'matching',
    label: 'settings.matching.review',
  },
  {
    id: 'setting-matching-fuzzy',
    section: 'matching',
    label: 'settings.matching.fuzzy',
  },
  {
    id: 'setting-matching-arcade-translation',
    section: 'matching',
    label: 'settings.matching.arcade',
  },
  {
    id: 'setting-matching-match-method',
    section: 'matching',
    label: 'settings.matching.method',
  },
  {
    id: 'setting-matching-max-hash-mb',
    section: 'matching',
    label: 'settings.matching.maxHash',
  },
  // Red
  {
    id: 'setting-network-concurrency',
    section: 'network',
    label: 'settings.network.concurrency',
  },
  {
    id: 'setting-network-timeout-secs',
    section: 'network',
    label: 'settings.network.timeout',
  },
  {
    id: 'setting-network-retries',
    section: 'network',
    label: 'settings.network.retries',
  },
  {
    id: 'setting-network-proxy-system',
    section: 'network',
    label: 'settings.network.proxies.system',
    hint: 'settings.network.proxy',
  },
  {
    id: 'setting-network-proxy-env',
    section: 'network',
    label: 'settings.network.proxies.env',
    hint: 'settings.network.proxy',
  },
  {
    id: 'setting-network-proxy-manual',
    section: 'network',
    label: 'settings.network.proxies.manual',
    hint: 'settings.network.proxy',
  },
  {
    id: 'setting-network-proxy-url',
    section: 'network',
    label: 'settings.network.proxyUrl',
  },
  {
    id: 'setting-network-cache-max-mb',
    section: 'network',
    label: 'settings.network.cacheMax',
  },
  {
    id: 'setting-network-index-refresh-hours',
    section: 'network',
    label: 'settings.network.refresh',
  },
  // Seguridad
  {
    id: 'setting-safety-backup',
    section: 'safety',
    label: 'settings.safety.backup',
    hint: 'settings.safety.backupHint',
  },
  {
    id: 'setting-safety-backup-dir',
    section: 'safety',
    label: 'settings.safety.backupDir',
    hint: 'settings.safety.backupDirHint',
  },
  {
    id: 'setting-safety-dry-run-default',
    section: 'safety',
    label: 'settings.safety.dryRun',
  },
  {
    id: 'setting-safety-clean-apple-double',
    section: 'safety',
    label: 'settings.safety.appleDouble',
  },
  {
    id: 'setting-safety-undo-depth',
    section: 'safety',
    label: 'settings.safety.undoDepth',
  },
  // Avanzado
  {
    id: 'setting-advanced-log-level',
    section: 'advanced',
    label: 'settings.advanced.logLevel',
  },
  {
    id: 'setting-advanced-custom-dir',
    section: 'advanced',
    label: 'settings.advanced.customDir',
    hint: 'settings.advanced.customDirHint',
  },
];

/** Entries inside a fold (the trigger's count). */
export const foldCount = (fold: SettingsFold) =>
  SETTINGS_INDEX.filter((e) => e.fold === fold).length;
