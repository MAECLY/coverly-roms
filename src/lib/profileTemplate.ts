/** Mirror of `profiles::custom` for live hints and the mock backend. */
import {
  MEDIA,
  type FieldError,
  type FieldErrorCode,
  type Media,
  type PreviewExample,
  type ProfileFile,
  type ProfileInfo,
  type SlotFile,
} from '@/ipc/types';

export const TOKENS = [
  '{root}',
  '{system_dir}',
  '{rel_dir}',
  '{stem}',
  '{system_id}',
  '{libretro}',
] as const;

/** Tokens that expand to paths: only ever a whole segment. */
const PATH_TOKENS = ['{root}', '{system_dir}', '{rel_dir}'] as const;

export interface TemplateVars {
  root: string;
  system_dir: string;
  rel_dir: string;
  stem: string;
  system_id: string;
  libretro: string | null;
}

/** A few folders the preview knows (the app uses the full systems database). */
const PREVIEW_SYSTEMS: Record<string, { id: string; libretro: string }> = {
  sfc: {
    id: 'snes',
    libretro: 'Nintendo - Super Nintendo Entertainment System',
  },
  snes: {
    id: 'snes',
    libretro: 'Nintendo - Super Nintendo Entertainment System',
  },
  gba: { id: 'gba', libretro: 'Nintendo - Game Boy Advance' },
  gb: { id: 'gb', libretro: 'Nintendo - Game Boy' },
  fc: { id: 'nes', libretro: 'Nintendo - Nintendo Entertainment System' },
  nes: { id: 'nes', libretro: 'Nintendo - Nintendo Entertainment System' },
  md: { id: 'megadrive', libretro: 'Sega - Mega Drive - Genesis' },
  ps: { id: 'psx', libretro: 'Sony - PlayStation' },
};

/** Expands `template` segment by segment, without empty or doubled separators. */
export function renderTemplate(template: string, v: TemplateVars): string {
  const parts: string[] = [];
  for (const seg of template.split('/')) {
    if (!seg) continue;
    if (seg === '{root}' || seg === '{system_dir}' || seg === '{rel_dir}') {
      const value =
        seg === '{root}'
          ? v.root
          : seg === '{system_dir}'
            ? v.system_dir
            : v.rel_dir;
      parts.push(...value.split('/').filter(Boolean));
      continue;
    }
    // One pass: a value holding `{stem}` is never expanded again.
    const text = seg.replace(
      /\{(root|system_dir|rel_dir|stem|system_id|libretro)\}/g,
      (_, name: keyof TemplateVars) =>
        name === 'libretro' ? (v.libretro ?? '') : v[name],
    );
    if (text) parts.push(text);
  }
  return parts.join('/');
}

const hasControl = (s: string) => /\p{Cc}/u.test(s);

const plainName = (s: string) => {
  const t = s.trim();
  return (
    t.length > 0 &&
    t !== '.' &&
    t !== '..' &&
    !/[/\\:]/.test(t) &&
    !hasControl(t)
  );
};

const literalSegmentOk = (seg: string) => {
  const t = seg.trim();
  return (
    t.length > 0 &&
    t !== '.' &&
    t !== '..' &&
    !/[\\:]/.test(seg) &&
    !hasControl(seg)
  );
};

function templateError(slot: SlotFile): FieldErrorCode | null {
  const s = slot.template;
  if (!s.trim()) return 'required';
  const tokens = s.match(/\{[^}]*\}?/g) ?? [];
  if (tokens.some((t) => !(TOKENS as readonly string[]).includes(t)))
    return 'unknown';
  if (s.trim() !== s) return 'invalid';
  const segs = s.split('/');
  if (segs.length < 2 || !(segs[0] === '{root}' || segs[0] === '{system_dir}'))
    return 'invalid';
  for (const [i, seg] of segs.entries()) {
    if (PATH_TOKENS.some((p) => seg.includes(p))) {
      const whole = (PATH_TOKENS as readonly string[]).includes(seg);
      const allowedHere = i === 0 || seg === '{rel_dir}';
      if (!whole || !allowedHere) return 'invalid';
      continue;
    }
    if (!literalSegmentOk(seg)) return 'invalid';
  }
  const last = segs[segs.length - 1] ?? '';
  if (!last.includes('{stem}')) return 'invalid';
  const extOk =
    slot.format === 'png'
      ? last.endsWith('.png')
      : last.endsWith('.jpg') || last.endsWith('.jpeg');
  return extOk ? null : 'invalid';
}

const fe = (
  field: string,
  code: FieldErrorCode,
  min: number | null = null,
  max: number | null = null,
): FieldError => ({ field, code, min, max });

/** Like `validate_profile`: per-field problems (`slots.<media>.<field>`). */
export function validateProfile(
  p: ProfileFile,
  builtinIds: readonly string[],
): FieldError[] {
  const out: FieldError[] = [];
  if (!/^[a-z0-9][a-z0-9-]{0,39}$/.test(p.id)) out.push(fe('id', 'invalid'));
  else if (builtinIds.some((b) => b.toLowerCase() === p.id.toLowerCase()))
    out.push(fe('id', 'duplicate'));
  const name = p.name.trim();
  if (!name) out.push(fe('name', 'required'));
  else if ([...name].length > 60 || hasControl(name))
    out.push(fe('name', 'invalid'));
  if (
    p.roms_root &&
    (p.roms_root.startsWith('/') || !p.roms_root.split('/').every(plainName))
  )
    out.push(fe('roms_root', 'invalid'));
  if (!p.art_dirs.every(plainName)) out.push(fe('art_dirs', 'invalid'));
  // Rust keeps slots in a sorted map: report in key order.
  const keys = Object.keys(p.slots).sort();
  if (keys.length === 0) out.push(fe('slots', 'required'));
  for (const key of keys) {
    const slot = p.slots[key as Media];
    if (!(MEDIA as readonly string[]).includes(key) || !slot) {
      out.push(fe(`slots.${key}`, 'unknown'));
      continue;
    }
    const code = templateError(slot);
    if (code) out.push(fe(`slots.${key}.template`, code));
    if (slot.max_width !== null && slot.max_height === null)
      out.push(fe(`slots.${key}.max_height`, 'required'));
    if (slot.max_width === null && slot.max_height !== null)
      out.push(fe(`slots.${key}.max_width`, 'required'));
    for (const f of ['max_width', 'max_height'] as const) {
      const v = slot[f];
      if (v !== null && !(v >= 16 && v <= 4096))
        out.push(fe(`slots.${key}.${f}`, 'range', 16, 4096));
    }
  }
  const mirrors = Object.values(p.slots).map(
    (s) => s?.template.includes('{rel_dir}') ?? false,
  );
  if (mirrors.some(Boolean) && !mirrors.every(Boolean))
    out.push(fe('slots', 'order'));
  return out;
}

/** Where each valid slot puts the example ROM's art, library-relative. */
export function previewProfile(
  p: ProfileFile,
  ex: PreviewExample,
): Partial<Record<Media, string>> {
  const folder = ex.system_folder.trim();
  const sys = PREVIEW_SYSTEMS[folder.toLowerCase()];
  const file = ex.file_name.trim();
  const dot = file.lastIndexOf('.');
  const stem = dot > 0 ? file.slice(0, dot) : file;
  const root = '__library__';
  const systemDir = [root, ...p.roms_root.split('/').filter(Boolean), folder]
    .filter(Boolean)
    .join('/');
  const out: Partial<Record<Media, string>> = {};
  for (const m of MEDIA) {
    const slot = p.slots[m];
    if (!slot || templateError(slot)) continue;
    const path = renderTemplate(slot.template, {
      root,
      system_dir: systemDir,
      rel_dir: ex.rel_dir.trim().replace(/^\/+|\/+$/g, ''),
      stem,
      system_id: sys?.id ?? 'unknown',
      libretro: sys?.libretro ?? null,
    });
    const parts = path.split('/');
    // A hostile example (`..` folders) has no preview: never a path
    // outside the library.
    if (parts[0] !== root || parts.length < 2) continue;
    const rest = parts.slice(1);
    if (rest.some((x) => x === '.' || x === '..')) continue;
    out[m] = rest.join('/');
  }
  return out;
}

/** A built-in profile as an editable copy with `id` and display `name`. */
export function profileFromBuiltin(
  info: ProfileInfo,
  id: string,
  name: string,
): ProfileFile {
  return {
    ...structuredClone(info.profile),
    id,
    name,
  };
}
