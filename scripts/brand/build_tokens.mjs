#!/usr/bin/env node
/**
 * Build the CSS custom properties of the app and the landing page from the
 * design tokens, so both read one source of truth: design/tokens.json
 * (W3C DTCG). Zero dependencies.
 *
 *   node scripts/brand/build_tokens.mjs           write the generated files
 *   node scripts/brand/build_tokens.mjs --check   fail if they are stale or a
 *                                                 contrast pair drops below
 *                                                 its WCAG target
 *
 * Outputs (never edit them by hand):
 *   src/styles/tokens.css   app: imported first by src/styles/globals.css
 *   site/assets/tokens.css  landing: <link>ed before site/assets/site.css
 *
 * Emitted format (docs/08-brand/brand.md §8):
 *   - Every colour role is a full colour, `--cr-primary: #0e5e6f`, for plain
 *     CSS (`color: var(--cr-primary)`). Legacy names (`--cr-accent` ...) are
 *     the same colours under their old names.
 *   - App only: RGB channels, `--cr-primary-rgb: 14 94 111`, for Tailwind 3
 *     opacity modifiers (`rgb(var(--cr-primary-rgb) / <alpha-value>)`, see
 *     tailwind.config.ts), the shadcn/ui names as full colours
 *     (`--background: #f2f3ef`) and the primitive scales.
 *   - Light is the default; dark is emitted under the same selectors each
 *     surface already uses for its theme toggle.
 *   - Durations collapse to 0ms under prefers-reduced-motion: reduce.
 */
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

export const REPO = resolve(
  dirname(fileURLToPath(import.meta.url)),
  '..',
  '..',
);
export const SOURCE = join(REPO, 'design', 'tokens.json');
export const OUTPUTS = {
  app: join(REPO, 'src', 'styles', 'tokens.css'),
  site: join(REPO, 'site', 'assets', 'tokens.css'),
};

const EXT = 'com.coverly-roms';

/* Theme selectors, mirrored from the code that sets data-theme:
   - app: src/lib/theme.ts always sets data-theme once settings load; before
     that, the OS decides (no attribute yet).
   - landing: site/assets/theme.js sets data-theme to the resolved mode; a
     no-JS visitor follows the OS unless the page is forced light. */
const SURFACES = {
  app: {
    light: [':root'],
    lightScheme: 'light dark',
    forcedLight: ":root[data-theme='light']",
    dark: ":root[data-theme='dark']",
    media: ':root:not([data-theme])',
    mediaScheme: false,
    channels: true,
    shadcn: true,
    primitives: true,
  },
  site: {
    light: [':root'],
    lightScheme: 'light',
    forcedLight: null,
    dark: ":root[data-theme='dark']",
    media: ":root:not([data-theme='light'])",
    mediaScheme: true,
    channels: false,
    shadcn: false,
    primitives: false,
  },
};

// ── Reading the DTCG tree ─────────────────────────────────────────────

export function readTokens(path = SOURCE) {
  return JSON.parse(readFileSync(path, 'utf8'));
}

function lookup(tokens, ref) {
  const path = ref.replace(/^\{|\}$/g, '').split('.');
  let node = tokens;
  for (const key of path) {
    node = node?.[key];
    if (node === undefined)
      throw new Error(`unresolved token reference ${ref}`);
  }
  return node;
}

/** The final `$value` of a token, following `{a.b.c}` aliases. */
export function resolveValue(tokens, value, seen = new Set()) {
  if (typeof value === 'string' && /^\{[^}]+\}$/.test(value)) {
    if (seen.has(value)) throw new Error(`circular token reference ${value}`);
    seen.add(value);
    return resolveValue(tokens, lookup(tokens, value).$value, seen);
  }
  return value;
}

/** Child tokens of a group, in file order, skipping `$` keys. */
function entries(group) {
  return Object.entries(group).filter(([k]) => !k.startsWith('$'));
}

// ── Values ────────────────────────────────────────────────────────────

function hexOf(colour) {
  const hex = String(colour.hex || '').toLowerCase();
  if (!/^#[0-9a-f]{6}$/.test(hex)) {
    throw new Error(`colour without a 6-digit hex: ${JSON.stringify(colour)}`);
  }
  return hex;
}

function channels(hex) {
  return [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));
}

function percent(alpha) {
  return `${Math.round(alpha * 1000) / 10}%`;
}

/** A DTCG colour as CSS: hex when opaque, rgb() with alpha otherwise. */
function cssColour(colour) {
  const hex = hexOf(colour);
  const alpha = colour.alpha ?? 1;
  if (alpha >= 1) return hex;
  return `rgb(${channels(hex).join(' ')} / ${percent(alpha)})`;
}

function dim(v) {
  return `${v.value}${v.unit}`;
}

function shadow(layers) {
  return layers
    .map((l) => {
      const parts = [dim(l.offsetX), dim(l.offsetY), dim(l.blur)];
      if (l.spread.value !== 0) parts.push(dim(l.spread));
      return [
        ...parts.map((p) => (p === '0px' ? '0' : p)),
        cssColour(l.color),
      ].join(' ');
    })
    .join(', ');
}

function family(list) {
  return list
    .map((f) => (/^[\w-]+$/.test(f) && !/\d/.test(f) ? f : `'${f}'`))
    .join(', ');
}

function bezier(p) {
  return `cubic-bezier(${p.join(', ')})`;
}

function slug(key) {
  return key.replace(/\./g, '-');
}

// ── Colour themes ─────────────────────────────────────────────────────

/** One theme's colour roles: [{css, hex|value, rgb, shadcn?}], deduped. */
export function themeColours(tokens, mode) {
  const groups = tokens.color;
  const out = new Map();
  const add = (name, colour, extra = {}) => {
    const value = cssColour(colour);
    const prev = out.get(name);
    if (prev && prev.value !== value) {
      throw new Error(
        `${name} has two values in ${mode}: ${prev.value} / ${value}`,
      );
    }
    if (!prev) {
      out.set(name, {
        name,
        value,
        rgb:
          (colour.alpha ?? 1) >= 1 ? channels(hexOf(colour)).join(' ') : null,
        ...extra,
      });
    }
  };
  for (const [role, tok] of entries(groups[mode])) {
    add(
      tok.$extensions?.[EXT]?.css ?? `--cr-${role}`,
      resolveValue(tokens, tok.$value),
    );
  }
  for (const [name, tok] of entries(groups[`legacy-${mode}`])) {
    add(
      tok.$extensions?.[EXT]?.css ?? `--cr-${name}`,
      resolveValue(tokens, tok.$value),
    );
  }
  return [...out.values()];
}

/** shadcn/ui names of one theme: [{name: '--background', value}]. */
export function shadcnColours(tokens, mode) {
  return entries(tokens.color[`shadcn-${mode}`]).map(([name, tok]) => ({
    name: `--${name}`,
    value: cssColour(resolveValue(tokens, tok.$value)),
    role: tok.$extensions?.[EXT]?.css,
    tailwind: tok.$extensions?.[EXT]?.tailwind ?? name,
  }));
}

/** Tailwind colour map: shadcn names plus `cr-<role>`, all with alpha. */
export function tailwindColors(tokens) {
  const rgb = (css) => `rgb(var(${css}-rgb) / <alpha-value>)`;
  const colors = {};
  for (const c of shadcnColours(tokens, 'light'))
    colors[c.tailwind] = rgb(c.role);
  const cr = {};
  for (const c of themeColours(tokens, 'light')) {
    if (c.rgb) cr[c.name.replace(/^--cr-/, '')] = rgb(c.name);
  }
  colors.cr = cr;
  return colors;
}

// ── Contrast (WCAG 2) ─────────────────────────────────────────────────

function luminance(hex) {
  const [r, g, b] = channels(hex).map((c) => {
    const s = c / 255;
    return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

export function contrast(a, b) {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

const SURFACE_ROLES = ['bg', 'surface', 'surface-raised', 'surface-sunken'];
const TEXT_ROLES = [
  'text',
  'text-secondary',
  'primary',
  'manila-text',
  'success',
  'warning',
  'danger',
  'info',
];
const UI_ROLES = ['border-strong', 'slot-missing', 'ring'];
const FILL_PAIRS = [
  ['primary-fg', 'primary', 4.5],
  ['primary-subtle-fg', 'primary-subtle', 4.5],
  ['manila-fg', 'manila', 4.5],
  ['danger-fill-fg', 'danger-fill', 4.5],
  ['warning-fill-fg', 'warning-fill', 4.5],
  ['success', 'success-bg', 4.5],
  ['warning', 'warning-bg', 4.5],
  ['danger', 'danger-bg', 4.5],
  ['info', 'info-bg', 4.5],
  ['code-fg', 'code-bg', 4.5],
  ['code-muted', 'code-bg', 4.5],
  ['code-accent', 'code-bg', 4.5],
];

/** Brand §4.2 targets. Returns the failing pairs (empty when all pass). */
export function contrastFailures(tokens) {
  const failures = [];
  for (const mode of ['light', 'dark']) {
    const roles = Object.fromEntries(
      themeColours(tokens, mode).map((c) => [
        c.name.replace(/^--cr-/, ''),
        c.value,
      ]),
    );
    const pairs = [
      ...TEXT_ROLES.flatMap((fg) => SURFACE_ROLES.map((bg) => [fg, bg, 4.5])),
      ...UI_ROLES.flatMap((fg) => SURFACE_ROLES.map((bg) => [fg, bg, 3])),
      ...FILL_PAIRS,
    ];
    for (const [fg, bg, need] of pairs) {
      if (!roles[fg] || !roles[bg]) {
        failures.push(`${mode}: --cr-${fg} or --cr-${bg} is missing`);
        continue;
      }
      const got = contrast(roles[fg], roles[bg]);
      if (got < need) {
        failures.push(
          `${mode}: --cr-${fg} on --cr-${bg} is ${got.toFixed(2)}:1, needs ${need}:1`,
        );
      }
    }
  }
  return failures;
}

// ── CSS writer ────────────────────────────────────────────────────────

function staticDecls(tokens, surface) {
  const lines = [];
  const decl = (name, value) => lines.push([name, value]);
  if (surface.primitives) {
    for (const [scale, steps] of entries(tokens.color.primitive)) {
      for (const [step, tok] of entries(steps)) {
        decl(`--cr-${scale}-${step}`, cssColour(tok.$value));
      }
    }
  }
  for (const [name, tok] of entries(tokens.font.family)) {
    decl(`--cr-font-${name}`, family(tok.$value));
  }
  for (const [name, tok] of entries(tokens.font.weight)) {
    decl(`--cr-font-weight-${name}`, String(tok.$value));
  }
  for (const [group, styles] of entries(tokens.type)) {
    for (const [name, tok] of entries(styles)) {
      const v = tok.$value;
      const fam = String(v.fontFamily).replace(
        /^\{font\.family\.([\w-]+)\}$/,
        'var(--cr-font-$1)',
      );
      decl(
        `--cr-type-${group}-${name}`,
        `${v.fontWeight} ${dim(v.fontSize)}/${v.lineHeight} ${fam}`,
      );
      if (v.letterSpacing && v.letterSpacing.value !== 0) {
        decl(`--cr-type-${group}-${name}-tracking`, dim(v.letterSpacing));
      }
    }
  }
  for (const [name, tok] of entries(tokens.radius)) {
    if (name === 'icon') continue; // app icon geometry, not UI
    decl(`--cr-radius-${name}`, dim(tok.$value));
  }
  for (const [name, tok] of entries(tokens.control))
    decl(`--cr-control-${name}`, dim(tok.$value));
  // JSON.parse puts integer keys ("1") before others ("0.5"): sort by size.
  const space = entries(tokens.space).sort(
    ([, a], [, b]) => a.$value.value - b.$value.value,
  );
  for (const [name, tok] of space)
    decl(`--cr-space-${slug(name)}`, dim(tok.$value));
  for (const [name, tok] of entries(tokens['border-width'])) {
    decl(`--cr-border-width-${name}`, dim(tok.$value));
  }
  for (const [name, tok] of entries(tokens.duration))
    decl(`--cr-duration-${name}`, dim(tok.$value));
  for (const [name, tok] of entries(tokens.easing))
    decl(`--cr-ease-${name}`, bezier(tok.$value));
  return lines;
}

function themeDecls(tokens, surface, mode) {
  const lines = [];
  for (const c of themeColours(tokens, mode)) {
    lines.push([c.name, c.value]);
    if (surface.channels && c.rgb) lines.push([`${c.name}-rgb`, c.rgb]);
  }
  if (surface.shadcn) {
    for (const c of shadcnColours(tokens, mode)) lines.push([c.name, c.value]);
  }
  for (const level of ['raised', 'overlay']) {
    lines.push([
      `--cr-shadow-${level}`,
      shadow(tokens.shadow[`${level}-${mode}`].$value),
    ]);
  }
  return lines;
}

/* Matches what Prettier prints (printWidth 80): a declaration that does not
   fit on one line breaks after the colon; a list that still does not fit
   fills each following line with as many items as fit. */
function formatDecl([name, value], indent) {
  const pad = ' '.repeat(indent);
  const one = `${pad}${name}: ${value};`;
  if (one.length <= 80) return one;
  const inner = `${pad}  `;
  const items = value.split(', ');
  const lines = [];
  let line = inner;
  items.forEach((item, i) => {
    const piece = `${item}${i < items.length - 1 ? ',' : ';'}`;
    if (line === inner) line += piece;
    else if (`${line} ${piece}`.length <= 80) line += ` ${piece}`;
    else {
      lines.push(line);
      line = inner + piece;
    }
  });
  lines.push(line);
  return `${pad}${name}:\n${lines.join('\n')}`;
}

function rule(selectors, decls, indent = 0) {
  const pad = ' '.repeat(indent);
  const body = decls.map((d) => formatDecl(d, indent + 2)).join('\n');
  return `${pad}${selectors.join(`,\n${pad}`)} {\n${body}\n${pad}}`;
}

function header(kind) {
  const where =
    kind === 'app'
      ? 'Imported first by src/styles/globals.css.'
      : '<link>ed before assets/site.css on every page.';
  return `/* GENERATED by scripts/brand/build_tokens.mjs from design/tokens.json.
   Do not edit: change the JSON and run \`pnpm tokens\` (or \`make tokens\`).
   ${where}
   --cr-<role> is a full colour; light is the default, dark follows the
   theme toggle. See docs/08-brand/brand.md §8. */`;
}

export function buildCss(tokens, kind) {
  const surface = SURFACES[kind];
  if (!surface) throw new Error(`unknown output ${kind}`);
  const blocks = [header(kind)];
  const light = [
    ...staticDecls(tokens, surface),
    ...themeDecls(tokens, surface, 'light'),
    ['color-scheme', surface.lightScheme],
  ];
  blocks.push(rule(surface.light, light));
  if (surface.forcedLight)
    blocks.push(rule([surface.forcedLight], [['color-scheme', 'light']]));
  const dark = themeDecls(tokens, surface, 'dark');
  const darkScheme = [...dark, ['color-scheme', 'dark']];
  if (kind === 'app') {
    blocks.push(rule([surface.dark], darkScheme));
    blocks.push(
      `@media (prefers-color-scheme: dark) {\n${rule([surface.media], surface.mediaScheme ? darkScheme : dark, 2)}\n}`,
    );
  } else {
    // The landing lists the OS default first, then the forced choice.
    blocks.push(
      `@media (prefers-color-scheme: dark) {\n${rule([surface.media], surface.mediaScheme ? darkScheme : dark, 2)}\n}`,
    );
    blocks.push(rule([surface.dark], darkScheme));
  }
  const still = entries(tokens.duration).map(([name]) => [
    `--cr-duration-${name}`,
    '0ms',
  ]);
  blocks.push(
    `@media (prefers-reduced-motion: reduce) {\n${rule([':root'], still, 2)}\n}`,
  );
  return `${blocks.join('\n\n')}\n`;
}

/** Every generated file: {absolute path: contents}. Pure and deterministic. */
export function buildAll(tokens) {
  return Object.fromEntries(
    Object.entries(OUTPUTS).map(([kind, path]) => [
      path,
      buildCss(tokens, kind),
    ]),
  );
}

/** What is on disk now for each generated file (null when missing). */
export function readOutputs() {
  return Object.fromEntries(
    Object.values(OUTPUTS).map((path) => {
      try {
        return [path, readFileSync(path, 'utf8')];
      } catch {
        return [path, null];
      }
    }),
  );
}

// ── CLI ───────────────────────────────────────────────────────────────

function main(argv) {
  const check = argv.includes('--check');
  const tokens = readTokens();
  const problems = contrastFailures(tokens);
  for (const p of problems) console.error(`contrast: ${p}`);
  const files = buildAll(tokens);
  let stale = 0;
  for (const [path, css] of Object.entries(files)) {
    const rel = relative(REPO, path);
    let current = null;
    try {
      current = readFileSync(path, 'utf8');
    } catch {
      current = null;
    }
    if (current === css) {
      if (!check) console.log(`up to date ${rel}`);
      continue;
    }
    if (check) {
      console.error(
        `stale: ${rel} differs from design/tokens.json (run pnpm tokens)`,
      );
      stale += 1;
    } else {
      writeFileSync(path, css);
      console.log(`wrote ${rel}`);
    }
  }
  if (problems.length || stale) return 1;
  if (check)
    console.log('tokens: generated files are up to date, contrast targets met');
  return 0;
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(resolve(process.argv[1])).href
) {
  process.exitCode = main(process.argv.slice(2));
}
