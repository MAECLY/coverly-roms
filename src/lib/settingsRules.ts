/**
 * Mirror of `coverly_core::settings` for the mock backend and for live
 * hints: defaults, ranges, wire shapes, normalisation, validation, merge
 * patches. The Rust core is the authority; `contract.test.ts` pins the
 * defaults and ranges to the shared sample.
 */
import {
  MEDIA,
  REGIONS,
  STRATEGIES,
  type FieldError,
  type FieldErrorCode,
  type Settings,
  type SettingsGroup,
  type SourceId,
} from '@/ipc/types';
import {
  DEDUPE_ACTIONS,
  DEDUPE_LEVELS,
  KEEP_RULES,
  ORGANIZE_DEDUPE_MODES,
} from '@/ipc/dedupeTypes';

export interface Range {
  field: string;
  min: number;
  max: number;
}

export const KNOWN_SOURCES: readonly SourceId[] = [
  'libretro',
  'steamgriddb',
  'screenscraper',
];
/** Sources the mock build can use (mirrors Rust without ScreenScraper dev
 * credentials); the others are normalised off. */
export const AVAILABLE_SOURCES: readonly SourceId[] = [
  'libretro',
  'steamgriddb',
];

/** Built-in profile ids (`builtin_profiles` in Rust). */
export const BUILTIN_LAYOUTS: readonly string[] = [
  'anbernic-stock',
  'garlicos',
  'onion',
  'muos',
  'minui',
  'esde',
  'batocera',
  'retroarch',
];

export const DEFAULT_SETTINGS: Settings = {
  version: 2,
  general: {
    language: 'auto',
    theme: 'system',
    check_updates_on_launch: true,
    update_channel: 'stable',
  },
  library: {
    default_profile: null,
    ignored_dirs: ['bios', 'saves', 'states', 'Shortcuts'],
    ignored_exts: [
      'txt',
      'xml',
      'json',
      'cfg',
      'ini',
      'db',
      'png',
      'jpg',
      'jpeg',
      'webp',
      'bmp',
      'srm',
      'sav',
      'state',
      'pdf',
      'md',
      'nfo',
      'log',
      'dat',
    ],
    include_hidden: false,
    follow_symlinks: false,
    recent: [],
  },
  art: {
    media: ['cover', 'screenshot', 'title'],
    single_slot_media: 'cover',
    output_format: 'profile',
    max_size: {},
    keep_aspect: true,
    jpeg_quality: 90,
    overwrite: 'never',
  },
  matching: {
    strategy: 'rom-region',
    regions: ['rom', 'world', 'usa', 'europe', 'japan'],
    languages: ['en'],
    auto_threshold: 0.9,
    review_threshold: 0.65,
    fuzzy: true,
    arcade_translation: true,
    match_method: 'hash-then-name',
    max_hash_mb: 64,
  },
  sources: {
    order: [
      { id: 'libretro', enabled: true },
      { id: 'steamgriddb', enabled: false },
      { id: 'screenscraper', enabled: false },
    ],
  },
  network: {
    concurrency: 4,
    timeout_secs: 30,
    retries: 3,
    proxy: 'system',
    proxy_url: '',
    cache_max_mb: 2048,
    index_refresh_hours: 48,
  },
  safety: {
    backup_before_overwrite: true,
    backup_dir: null,
    dry_run_default: true,
    clean_apple_double: true,
    undo_depth: 10,
  },
  advanced: { log_level: 'info', custom_dir: null },
  organize: {
    default_layout: 'anbernic-stock',
    nest_in_roms_root: true,
    mode: 'auto',
    names: 'keep',
    download_dats: true,
    auto_threshold: 0.85,
    review_threshold: 0.5,
    companion_exts: [
      'sav',
      'srm',
      'rtc',
      'state*',
      'cht',
      'ips',
      'bps',
      'ups',
      'txt',
      'nfo',
      'png',
      'jpg',
    ],
    folder_names: {},
    arcade_order: {},
    recent_seconds: 60,
    keep_organize_journals_days: 90,
  },
  dedupe: {
    levels: ['exact', 'format', 'version'],
    one_game_one_rom: false,
    action: 'quarantine',
    quarantine_dir: null,
    quarantine_name: null,
    keep_rules: KEEP_RULES.map((rule) => ({ rule, on: true })),
    container_order: {
      '*': { arcade: ['zip'] },
      'anbernic-stock': { disc: ['chd', 'pbp', 'cue', 'iso', 'img', 'bin'] },
      batocera: { disc: ['chd', 'cue', 'pbp', 'iso', 'img', 'bin'] },
      esde: { disc: ['chd', 'cue', 'pbp', 'iso', 'img', 'bin'] },
      garlicos: { disc: ['chd', 'pbp', 'cue', 'iso', 'img', 'bin'] },
      minui: { disc: ['chd', 'pbp', 'cue', 'iso', 'img', 'bin'] },
      muos: { disc: ['chd', 'pbp', 'cue', 'iso', 'img', 'bin'] },
      onion: { disc: ['chd', 'pbp', 'cue', 'iso', 'img', 'bin'] },
      retroarch: { disc: ['chd', 'cue', 'pbp', 'iso', 'img', 'bin'] },
    },
    exclude_systems: ['ports', 'bios', 'apps'],
    max_dat_downloads: 12,
    organize_step: 'set-aside',
  },
};

export const RANGES: readonly Range[] = [
  { field: 'art.jpeg_quality', min: 1, max: 100 },
  { field: 'art.max_size.width', min: 16, max: 4096 },
  { field: 'art.max_size.height', min: 16, max: 4096 },
  { field: 'matching.auto_threshold', min: 0.3, max: 1 },
  { field: 'matching.review_threshold', min: 0.3, max: 1 },
  { field: 'matching.max_hash_mb', min: 1, max: 4096 },
  { field: 'network.concurrency', min: 1, max: 6 },
  { field: 'network.timeout_secs', min: 5, max: 300 },
  { field: 'network.retries', min: 0, max: 10 },
  { field: 'network.cache_max_mb', min: 64, max: 102400 },
  { field: 'network.index_refresh_hours', min: 1, max: 720 },
  { field: 'safety.undo_depth', min: 1, max: 100 },
  { field: 'organize.auto_threshold', min: 0, max: 1 },
  { field: 'organize.review_threshold', min: 0, max: 1 },
  { field: 'organize.recent_seconds', min: 0, max: 3600 },
  { field: 'organize.keep_organize_journals_days', min: 1, max: 3650 },
  { field: 'dedupe.max_dat_downloads', min: 0, max: 100 },
];

export const GROUPS: readonly SettingsGroup[] = [
  'general',
  'library',
  'art',
  'matching',
  'sources',
  'network',
  'safety',
  'advanced',
  'organize',
  'dedupe',
];

export function cloneSettings(s: Settings): Settings {
  return structuredClone(s);
}

export const rangeOf = (field: string): Range | undefined =>
  RANGES.find((r) => r.field === field);

const isObject = (v: unknown): v is Record<string, unknown> =>
  typeof v === 'object' && v !== null && !Array.isArray(v);

/** RFC 7386 merge patch; returns a new value (`__proto__` keys are dropped). */
export function mergePatch<T>(target: T, patch: unknown): T {
  if (!isObject(patch)) return structuredClone(patch) as T;
  const out: Record<string, unknown> = isObject(target) ? { ...target } : {};
  for (const [k, v] of Object.entries(patch)) {
    if (k === '__proto__') continue;
    if (v === null) delete out[k];
    else out[k] = mergePatch(out[k], v);
  }
  return out as T;
}

/** Dotted paths of the leaves of a patch, in order. */
export function leafPaths(patch: unknown, prefix = ''): string[] {
  if (!isObject(patch) || Object.keys(patch).length === 0)
    return prefix ? [prefix] : [];
  return Object.entries(patch).flatMap(([k, v]) =>
    leafPaths(v, prefix ? `${prefix}.${k}` : k),
  );
}

/* Wire shapes: what serde would accept for each field. */

type Kind = (v: unknown) => boolean;
const isBool: Kind = (v) => typeof v === 'boolean';
const isStr: Kind = (v) => typeof v === 'string';
const isOptStr: Kind = (v) => v === null || typeof v === 'string';
const isUint =
  (max = 0xffff_ffff): Kind =>
  (v) =>
    typeof v === 'number' && Number.isInteger(v) && v >= 0 && v <= max;
const isNum: Kind = (v) => typeof v === 'number' && Number.isFinite(v);
const oneOf =
  (values: readonly string[]): Kind =>
  (v) =>
    typeof v === 'string' && values.includes(v);
const listOf =
  (item: Kind): Kind =>
  (v) =>
    Array.isArray(v) && v.every(item);
const isMedia = oneOf(MEDIA);
const isRecent: Kind = (v) =>
  isObject(v) &&
  isStr(v.path) &&
  isStr(v.name) &&
  isStr(v.profile_id) &&
  isUint()(v.roms) &&
  typeof v.opened_at === 'number' &&
  Number.isInteger(v.opened_at);
const isMaxSizes: Kind = (v) =>
  isObject(v) &&
  Object.values(v).every(
    (s) => isObject(s) && isUint()(s.width) && isUint()(s.height),
  );
const isSourceEntry: Kind = (v) =>
  isObject(v) && isStr(v.id) && isBool(v.enabled);
const isFolderNames: Kind = (v) =>
  isObject(v) &&
  Object.values(v).every((m) => isObject(m) && Object.values(m).every(isStr));
const isArcadeOrders: Kind = (v) =>
  isObject(v) && Object.values(v).every(listOf(isStr));
const isRuleToggle: Kind = (v) =>
  isObject(v) && oneOf(KEEP_RULES)(v.rule) && isBool(v.on);
const isContainerOrders: Kind = (v) =>
  isObject(v) &&
  Object.values(v).every(
    (m) => isObject(m) && Object.values(m).every(listOf(isStr)),
  );

const SHAPES: { [G in SettingsGroup]: Record<keyof Settings[G], Kind> } = {
  general: {
    language: oneOf(['auto', 'en', 'es']),
    theme: oneOf(['system', 'light', 'dark']),
    check_updates_on_launch: isBool,
    update_channel: oneOf(['stable', 'beta']),
  },
  library: {
    default_profile: isOptStr,
    ignored_dirs: listOf(isStr),
    ignored_exts: listOf(isStr),
    include_hidden: isBool,
    follow_symlinks: isBool,
    recent: listOf(isRecent),
  },
  art: {
    media: listOf(isMedia),
    single_slot_media: isMedia,
    output_format: oneOf(['profile', 'png', 'jpg']),
    max_size: isMaxSizes,
    keep_aspect: isBool,
    jpeg_quality: isUint(255),
    overwrite: oneOf(['never', 'ask', 'backup']),
  },
  matching: {
    strategy: oneOf(STRATEGIES),
    regions: listOf(isStr),
    languages: listOf(isStr),
    auto_threshold: isNum,
    review_threshold: isNum,
    fuzzy: isBool,
    arcade_translation: isBool,
    match_method: oneOf(['hash-then-name', 'hash-only', 'name-only']),
    max_hash_mb: isUint(),
  },
  sources: { order: listOf(isSourceEntry) },
  network: {
    concurrency: isUint(),
    timeout_secs: isUint(),
    retries: isUint(),
    proxy: oneOf(['system', 'env', 'manual']),
    proxy_url: isStr,
    cache_max_mb: isUint(),
    index_refresh_hours: isUint(),
  },
  safety: {
    backup_before_overwrite: isBool,
    backup_dir: isOptStr,
    dry_run_default: isBool,
    clean_apple_double: isBool,
    undo_depth: isUint(),
  },
  advanced: {
    log_level: oneOf(['error', 'warn', 'info', 'debug']),
    custom_dir: isOptStr,
  },
  organize: {
    default_layout: isStr,
    nest_in_roms_root: isBool,
    mode: oneOf(['auto', 'move', 'copy']),
    names: oneOf(['keep', 'catalogue']),
    download_dats: isBool,
    auto_threshold: isNum,
    review_threshold: isNum,
    companion_exts: listOf(isStr),
    folder_names: isFolderNames,
    arcade_order: isArcadeOrders,
    recent_seconds: isUint(),
    keep_organize_journals_days: isUint(),
  },
  dedupe: {
    levels: listOf(oneOf(DEDUPE_LEVELS)),
    one_game_one_rom: isBool,
    action: oneOf(DEDUPE_ACTIONS),
    quarantine_dir: isOptStr,
    quarantine_name: isOptStr,
    keep_rules: listOf(isRuleToggle),
    container_order: isContainerOrders,
    exclude_systems: listOf(isStr),
    max_dat_downloads: isUint(),
    organize_step: oneOf(ORGANIZE_DEDUPE_MODES),
  },
};

const fieldsOf = (g: SettingsGroup) =>
  SHAPES[g] as unknown as Record<string, Kind>;

/**
 * Settings from any value, field by field: known fields of the right shape
 * are kept, a missing one is its default, unknown ones are dropped (like
 * serde with `#[serde(default)]`). `bad` lists the fields (or whole
 * groups) whose value had the wrong shape and got the default.
 */
export function shapeSettings(raw: unknown): {
  settings: Settings;
  bad: string[];
} {
  const settings = cloneSettings(DEFAULT_SETTINGS);
  const bad: string[] = [];
  if (!isObject(raw)) return { settings, bad };
  for (const g of GROUPS) {
    const user = raw[g];
    if (user === undefined) continue;
    if (!isObject(user)) {
      bad.push(g);
      continue;
    }
    const group = settings[g] as unknown as Record<string, unknown>;
    for (const [k, ok] of Object.entries(fieldsOf(g))) {
      if (!Object.prototype.hasOwnProperty.call(user, k)) continue;
      const v = user[k];
      if (ok(v)) group[k] = structuredClone(v);
      else bad.push(`${g}.${k}`);
    }
  }
  return { settings, bad };
}

/**
 * Problems a patch has before it is even merged, like `with_patch` in
 * Rust: it must be an object of groups, each an object (or `null` to
 * reset it), and every value must have the field's wire shape.
 */
export function patchShapeErrors(patch: unknown): FieldError[] {
  if (!isObject(patch)) return [err('', 'invalid')];
  const out: FieldError[] = [];
  for (const [g, v] of Object.entries(patch)) {
    if (!(isObject(v) || v === null)) out.push(err(g, 'invalid'));
  }
  if (out.length) return out;
  for (const [g, v] of Object.entries(patch)) {
    if (!isObject(v) || !(GROUPS as readonly string[]).includes(g)) continue;
    const fields = fieldsOf(g as SettingsGroup);
    for (const [k, value] of Object.entries(v)) {
      const ok = fields[k];
      if (!ok || value === null) continue;
      if (k === 'max_size' && isObject(value)) {
        for (const [id, size] of Object.entries(value)) {
          if (size !== null && !isMaxSizes({ [id]: size })) {
            out.push(err(`${g}.${k}.${id}`, 'invalid'));
          }
        }
        continue;
      }
      if (!ok(value)) out.push(err(`${g}.${k}`, 'invalid'));
    }
  }
  return out;
}

const err = (field: string, code: FieldErrorCode, r?: Range): FieldError => ({
  field,
  code,
  min: r ? r.min : null,
  max: r ? r.max : null,
});

function check(out: FieldError[], rule: string, field: string, value: number) {
  const r = rangeOf(rule);
  if (r && !(value >= r.min && value <= r.max))
    out.push(err(field, 'range', r));
}

const isAbsolute = (p: string) =>
  p.startsWith('/') || /^[A-Za-z]:[\\/]/.test(p);

/** Every problem, in the order of `Settings::validate`. */
export function validateSettings(
  s: Settings,
  known: readonly string[],
): FieldError[] {
  const out: FieldError[] = [];
  const simple = (f: string, v: number) => check(out, f, f, v);
  if (
    s.library.default_profile !== null &&
    !known.includes(s.library.default_profile)
  ) {
    out.push(err('library.default_profile', 'unknown'));
  }
  if (s.art.media.length === 0) out.push(err('art.media', 'required'));
  for (const [id, size] of Object.entries(s.art.max_size)) {
    if (!known.includes(id)) {
      out.push(err(`art.max_size.${id}`, 'unknown'));
      continue;
    }
    check(out, 'art.max_size.width', `art.max_size.${id}.width`, size.width);
    check(out, 'art.max_size.height', `art.max_size.${id}.height`, size.height);
  }
  simple('art.jpeg_quality', s.art.jpeg_quality);
  const m = s.matching;
  const regions = m.regions.map((r) => String(r).trim().toLowerCase());
  if (regions.length === 0) out.push(err('matching.regions', 'required'));
  else if (
    !regions.every(
      (r) => r === 'rom' || (REGIONS as readonly string[]).includes(r),
    )
  )
    out.push(err('matching.regions', 'invalid'));
  else if (new Set(regions).size !== regions.length)
    out.push(err('matching.regions', 'duplicate'));
  if (
    m.languages.length > 10 ||
    !m.languages.every((l) => /^[a-z]{2}$/.test(l))
  )
    out.push(err('matching.languages', 'invalid'));
  simple('matching.auto_threshold', m.auto_threshold);
  simple('matching.review_threshold', m.review_threshold);
  if (m.review_threshold > m.auto_threshold)
    out.push(err('matching.review_threshold', 'order'));
  simple('matching.max_hash_mb', m.max_hash_mb);
  const ids = s.sources.order.map((e) => e.id);
  if (!ids.every((id) => KNOWN_SOURCES.includes(id)))
    out.push(err('sources.order', 'unknown'));
  else if (new Set(ids).size !== ids.length)
    out.push(err('sources.order', 'duplicate'));
  else if (!s.sources.order.some((e) => e.enabled))
    out.push(err('sources.order', 'required'));
  const n = s.network;
  simple('network.concurrency', n.concurrency);
  simple('network.timeout_secs', n.timeout_secs);
  simple('network.retries', n.retries);
  simple('network.cache_max_mb', n.cache_max_mb);
  simple('network.index_refresh_hours', n.index_refresh_hours);
  if (n.proxy === 'manual') {
    const url = n.proxy_url.trim();
    if (!url) out.push(err('network.proxy_url', 'required'));
    else if (!/^https?:\/\/\S*$/.test(url))
      out.push(err('network.proxy_url', 'invalid'));
  }
  const sf = s.safety;
  if (!sf.backup_before_overwrite)
    out.push(err('safety.backup_before_overwrite', 'invalid'));
  if (sf.backup_dir !== null && !isAbsolute(sf.backup_dir))
    out.push(err('safety.backup_dir', 'not-absolute'));
  simple('safety.undo_depth', sf.undo_depth);
  if (s.advanced.custom_dir !== null && !isAbsolute(s.advanced.custom_dir))
    out.push(err('advanced.custom_dir', 'not-absolute'));
  // `organize`: the mock has no system database, so folder names and
  // arcade orders are only checked for a known layout (Rust also checks
  // that they name real systems).
  const o = s.organize;
  // Built-in layouts always exist (Rust adds them to `known` too).
  const layouts = [...known, ...BUILTIN_LAYOUTS];
  const layoutKnown = (id: string) => layouts.includes(id);
  if (!layoutKnown(o.default_layout.trim()))
    out.push(err('organize.default_layout', 'unknown'));
  simple('organize.auto_threshold', o.auto_threshold);
  simple('organize.review_threshold', o.review_threshold);
  if (o.review_threshold > o.auto_threshold)
    out.push(err('organize.review_threshold', 'order'));
  simple('organize.recent_seconds', o.recent_seconds);
  simple('organize.keep_organize_journals_days', o.keep_organize_journals_days);
  for (const id of Object.keys(o.folder_names))
    if (!layoutKnown(id))
      out.push(err(`organize.folder_names.${id}`, 'unknown'));
  for (const [id, order] of Object.entries(o.arcade_order)) {
    if (!layoutKnown(id))
      out.push(err(`organize.arcade_order.${id}`, 'unknown'));
    else if (new Set(order).size !== order.length)
      out.push(err(`organize.arcade_order.${id}`, 'invalid'));
  }
  return out;
}

function uniq(list: string[]): string[] {
  return [...new Set(list)];
}

/** Like `Settings::normalise`: cleans what can be cleaned without asking. */
export function normaliseSettings(input: Settings): Settings {
  const s = cloneSettings(input);
  s.version = 2;
  const dirs: string[] = [];
  for (const raw of s.library.ignored_dirs) {
    const d = raw.trim();
    if (!d || d === '.' || d === '..' || /[\\/]/.test(d)) continue;
    if (dirs.some((o) => o.toLowerCase() === d.toLowerCase())) continue;
    dirs.push(d);
  }
  s.library.ignored_dirs = dirs;
  s.library.ignored_exts = uniq(
    s.library.ignored_exts
      .map((e) => e.trim().replace(/^\.+/, '').toLowerCase())
      .filter((e) => e.length > 0 && e.length <= 16 && /^[a-z0-9_-]+$/.test(e)),
  );
  s.library.recent = s.library.recent.slice(0, 10);
  s.art.media = MEDIA.filter((m) => s.art.media.includes(m));
  s.matching.regions = uniq(
    s.matching.regions
      .map((r) => String(r).trim().toLowerCase())
      .filter(Boolean),
  ) as Settings['matching']['regions'];
  s.matching.languages = uniq(
    s.matching.languages.map((l) => l.trim().toLowerCase()).filter(Boolean),
  );
  const order: Settings['sources']['order'] = [];
  for (const e of s.sources.order) {
    if (KNOWN_SOURCES.includes(e.id) && !order.some((o) => o.id === e.id)) {
      order.push({
        id: e.id,
        enabled: e.enabled && AVAILABLE_SOURCES.includes(e.id),
      });
    }
  }
  for (const id of KNOWN_SOURCES)
    if (!order.some((o) => o.id === id)) order.push({ id, enabled: false });
  s.sources.order = order;
  s.network.proxy_url = s.network.proxy_url.trim();
  s.organize.companion_exts = uniq(
    s.organize.companion_exts
      .map((e) => e.trim().replace(/^\.+/, '').toLowerCase())
      .filter(Boolean),
  );
  s.organize.default_layout = s.organize.default_layout.trim();
  s.dedupe.exclude_systems = uniq(
    s.dedupe.exclude_systems.map((e) => e.trim().toLowerCase()).filter(Boolean),
  );
  if (s.dedupe.quarantine_name !== null)
    s.dedupe.quarantine_name = s.dedupe.quarantine_name.trim();
  for (const rule of KEEP_RULES)
    if (!s.dedupe.keep_rules.some((t) => t.rule === rule))
      s.dedupe.keep_rules.push({ rule, on: true });
  return s;
}

/**
 * Stored JSON → settings, field by field (mock storage; Rust has its own
 * loader): a field of the wrong shape or that fails validation falls back
 * to its default (both thresholds when they are misordered). `fixed`
 * lists what fell back.
 */
export function tolerantSettings(
  raw: unknown,
  known: readonly string[],
): { settings: Settings; fixed: string[] } {
  const shaped = shapeSettings(raw);
  const fixed = [...shaped.bad];
  let s = normaliseSettings(shaped.settings);
  for (const e of validateSettings(s, known)) {
    const [g, f] = e.field.split('.') as [SettingsGroup, string];
    if (e.code === 'order' && g === 'organize') {
      s.organize.auto_threshold = DEFAULT_SETTINGS.organize.auto_threshold;
      s.organize.review_threshold = DEFAULT_SETTINGS.organize.review_threshold;
    } else if (e.code === 'order') {
      s.matching.auto_threshold = DEFAULT_SETTINGS.matching.auto_threshold;
      s.matching.review_threshold = DEFAULT_SETTINGS.matching.review_threshold;
    } else if (
      g === 'organize' &&
      (f === 'folder_names' || f === 'arcade_order')
    ) {
      // One layout's entry, like Rust: the other layouts' overrides stay.
      const layout = e.field.slice(`organize.${f}.`.length).split('.')[0] ?? '';
      if (f === 'folder_names') {
        const names = { ...s.organize.folder_names };
        delete names[layout];
        s.organize.folder_names = names;
      } else {
        const orders = { ...s.organize.arcade_order };
        delete orders[layout];
        s.organize.arcade_order = orders;
      }
    } else {
      const def = DEFAULT_SETTINGS[g] as unknown as Record<string, unknown>;
      (s[g] as unknown as Record<string, unknown>)[f] = structuredClone(def[f]);
    }
    const key = `${g}.${f}`;
    if (!fixed.includes(key)) fixed.push(key);
  }
  s = normaliseSettings(s);
  return { settings: s, fixed };
}

/** Like `Settings::needs_reopen`: the open library must be reopened. */
export function needsReopen(a: Settings, b: Settings): boolean {
  const lib = (s: Settings) =>
    JSON.stringify([
      s.library.ignored_dirs,
      s.library.ignored_exts,
      s.library.include_hidden,
      s.library.follow_symlinks,
    ]);
  const sizes = (s: Settings) =>
    JSON.stringify(
      Object.entries(s.art.max_size).sort(([x], [y]) =>
        x < y ? -1 : x > y ? 1 : 0,
      ),
    );
  return (
    lib(a) !== lib(b) ||
    a.art.single_slot_media !== b.art.single_slot_media ||
    a.art.output_format !== b.art.output_format ||
    sizes(a) !== sizes(b) ||
    a.advanced.custom_dir !== b.advanced.custom_dir
  );
}
