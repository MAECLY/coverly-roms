// Pure helpers for the system icon generator (scripts/icons/systems.mjs).
//
// The icons are RetroArch Assets, XMB "Monochrome" theme (CC BY 4.0,
// (c) libretro contributors), vendored at a pinned commit. These helpers
// recolour them to currentColor, clean them with SVGO, crop each viewBox to
// its drawing and render the generated TS module, README and contact sheet.
// No network access here, so Vitest can test every function directly.

import { optimize } from 'svgo';
import { svgPathBbox } from 'svg-path-bbox';

export const COMMIT = 'd9f969054dc7fbb6fa89519036d2b971e0855b51';
export const REPO_URL = 'https://github.com/libretro/retroarch-assets';
export const SOURCE_DIR = 'src/xmb/monochrome/';
export const SOURCE_BASE = `https://raw.githubusercontent.com/libretro/retroarch-assets/${COMMIT}/${SOURCE_DIR}`;

const CABINET = 'FBNeo - Arcade Games.svg';
const CABINET_AUDIT =
  'arcade cabinet in side profile, no marquee art, no marks';

/**
 * Coverly system id -> source file in the Monochrome theme. Every entry says
 * what the glyph depicts and carries a one-line audit written while looking
 * at the source: no maker logo, wordmark, badge or registered button symbol.
 * Removing an entry sends that system to the abbreviation fallback.
 */
export const SYSTEM_ICON_SOURCES = Object.freeze({
  a2600: {
    file: 'Atari - 2600.svg',
    depicts: 'pad',
    audit: 'joystick seen from above, plain knob, no marks',
  },
  a5200: {
    file: 'Atari - 5200.svg',
    depicts: 'pad',
    audit: 'keypad controller, plain keys, no marks',
  },
  a7800: {
    file: 'Atari - 7800.svg',
    depicts: 'pad',
    audit: 'joystick base with plain knob and two plain buttons, no marks',
  },
  a800: {
    file: 'Atari - 8-bit Family.svg',
    depicts: 'computer',
    audit: 'computer with keyboard, plain keys, no marks',
  },
  atarist: {
    file: 'Atari - ST.svg',
    depicts: 'computer',
    audit: 'computer with keyboard, plain keys, no marks',
  },
  lynx: {
    file: 'Atari - Lynx.svg',
    depicts: 'handheld',
    audit: 'landscape handheld, blank screen, no badge',
  },
  amiga: {
    file: 'Commodore - Amiga.svg',
    depicts: 'computer',
    audit: 'computer with keyboard, plain keys, no marks',
  },
  c64: {
    file: 'Commodore - 64.svg',
    depicts: 'computer',
    audit: 'keyboard computer, plain keys, no marks',
  },
  vic20: {
    file: 'Commodore - VIC-20.svg',
    depicts: 'computer',
    audit: 'keyboard computer, plain keys, no marks',
  },
  msx: {
    file: 'Microsoft - MSX.svg',
    depicts: 'computer',
    audit: 'keyboard computer, plain keys, boxed cursor key, no marks',
  },
  nes: {
    file: 'Nintendo - Nintendo Entertainment System.svg',
    depicts: 'pad',
    audit: 'rectangular pad, d-pad and plain round buttons, no marks',
  },
  fds: {
    file: 'Nintendo - Family Computer Disk System.svg',
    depicts: 'pad',
    audit: 'rectangular pad, d-pad and plain round buttons, no marks',
  },
  snes: {
    file: 'Nintendo - Super Nintendo Entertainment System.svg',
    depicts: 'pad',
    audit: 'rounded pad, plain dots, no marks',
  },
  n64: {
    file: 'Nintendo - Nintendo 64.svg',
    depicts: 'pad',
    audit: 'three-prong pad, plain buttons, no marks',
  },
  gb: {
    file: 'Nintendo - Game Boy.svg',
    depicts: 'handheld',
    audit: 'portrait handheld, blank screen, no badge',
  },
  gbc: {
    file: 'Nintendo - Game Boy Color.svg',
    depicts: 'handheld',
    audit: 'portrait handheld, blank screen, no badge',
  },
  gba: {
    file: 'Nintendo - Game Boy Advance.svg',
    depicts: 'handheld',
    audit: 'landscape handheld, blank screen, no badge',
  },
  nds: {
    file: 'Nintendo - Nintendo DS.svg',
    depicts: 'handheld',
    audit: 'open clamshell handheld, two blank screens, no badge',
  },
  gw: {
    file: 'Nintendo - Game & Watch.svg',
    depicts: 'handheld',
    audit: 'landscape handheld, blank screen, plain buttons, no badge',
  },
  virtualboy: {
    file: 'Nintendo - Virtual Boy.svg',
    depicts: 'pad',
    audit: 'double-grip pad, two d-pads, plain buttons, no marks',
  },
  pokemini: {
    file: 'Nintendo - Pokemon Mini.svg',
    depicts: 'handheld',
    audit: 'small handheld, blank screen, d-pad, no character art or badge',
  },
  mastersystem: {
    file: 'Sega - Master System - Mark III.svg',
    depicts: 'console',
    audit: 'console top with plain dial and two plain buttons, no marks',
  },
  megadrive: {
    file: 'Sega - Mega Drive - Genesis.svg',
    depicts: 'pad',
    audit: 'crescent pad, plain dots, no marks',
  },
  segacd: {
    file: 'Sega - Mega-CD - Sega CD.svg',
    depicts: 'pad',
    audit: 'crescent pad, plain dots, no marks',
  },
  sega32x: {
    file: 'Sega - 32X.svg',
    depicts: 'pad',
    audit: 'crescent pad, plain dots, no marks',
  },
  saturn: {
    file: 'Sega - Saturn.svg',
    depicts: 'pad',
    audit: 'rounded pad, plain dots, no marks',
  },
  dreamcast: {
    file: 'Sega - Dreamcast.svg',
    depicts: 'pad',
    audit:
      'pad with plain round buttons, blank memory-card window, outline start triangle, no swirl',
  },
  gamegear: {
    file: 'Sega - Game Gear.svg',
    depicts: 'handheld',
    audit: 'landscape handheld, blank screen, no badge',
  },
  neogeo: {
    file: 'SNK - Neo Geo.svg',
    depicts: 'pad',
    audit: 'arcade stick box, plain ball top and buttons, no marks',
  },
  neocd: {
    file: 'SNK - Neo Geo CD.svg',
    depicts: 'pad',
    audit: 'rounded pad, plain buttons, no marks',
  },
  ngp: {
    file: 'SNK - Neo Geo Pocket.svg',
    depicts: 'handheld',
    audit: 'landscape handheld, blank screen, no badge',
  },
  ngpc: {
    file: 'SNK - Neo Geo Pocket Color.svg',
    depicts: 'handheld',
    audit: 'landscape handheld, blank screen, no badge',
  },
  pcengine: {
    file: 'NEC - PC Engine - TurboGrafx 16.svg',
    depicts: 'pad',
    audit: 'rectangular pad, plain buttons, no marks',
  },
  pcenginecd: {
    file: 'NEC - PC Engine CD - TurboGrafx-CD.svg',
    depicts: 'pad',
    audit: 'rectangular pad, plain buttons, no marks',
  },
  psx: {
    file: 'Sony - PlayStation.svg',
    depicts: 'pad',
    audit: 'pad with grips, plain round face buttons, no face-button symbols',
  },
  psp: {
    file: 'Sony - PlayStation Portable.svg',
    depicts: 'handheld',
    audit: 'landscape handheld, blank screen, plain round buttons, no symbols',
  },
  wonderswan: {
    file: 'Bandai - WonderSwan.svg',
    depicts: 'handheld',
    audit: 'landscape handheld, blank screen, plain button clusters, no badge',
  },
  wonderswancolor: {
    file: 'Bandai - WonderSwan Color.svg',
    depicts: 'handheld',
    audit: 'landscape handheld, blank screen, plain button clusters, no badge',
  },
  fbneo: { file: CABINET, depicts: 'cabinet', audit: CABINET_AUDIT },
  varcade: {
    file: CABINET,
    depicts: 'cabinet',
    audit: `${CABINET_AUDIT} (shared cabinet proxy)`,
  },
  pgm2: {
    file: CABINET,
    depicts: 'cabinet',
    audit: `${CABINET_AUDIT} (shared cabinet proxy)`,
  },
});

// ---------------------------------------------------------------------------
// Normalisation

/**
 * SVGO plugin, run first: drop clip paths. Every clip in the set is the full
 * 64x64 canvas, and the cropped viewBox (clamped to the source viewBox) does
 * the same job. Ids are stripped later, so references would dangle anyway,
 * and inline icons must not share ids on one page.
 */
const dropClipPaths = {
  name: 'coverlyDropClipPaths',
  fn: () => ({
    element: {
      enter: (node, parent) => {
        if (node.name === 'clipPath') {
          parent.children = parent.children.filter((c) => c !== node);
          return;
        }
        delete node.attributes['clip-path'];
      },
    },
  }),
};

const PAINT = /\b(fill|stroke)="([^"]*)"/g;

/** svgo pass + fills -> currentColor + strip ids/classes/styles. Throws on a dirty result. */
export function normaliseSvg(svg) {
  const { data } = optimize(svg, {
    multipass: true,
    plugins: [
      dropClipPaths,
      'preset-default',
      'removeDimensions',
      'convertStyleToAttrs',
      {
        name: 'removeAttrs',
        params: { attrs: ['id', 'class', 'data-.*', 'style'] },
      },
      'removeStyleElement',
    ],
  });
  const out = data.replace(PAINT, (_m, prop, value) =>
    value === 'none' || value === 'currentColor'
      ? `${prop}="${value}"`
      : `${prop}="currentColor"`,
  );
  assertClean('svg', out);
  return out;
}

function rootTag(svg) {
  const m = /<svg\b[^>]*>/.exec(svg);
  if (!m) throw new Error('not an <svg> document');
  return m[0];
}

function attr(tag, name) {
  const m = new RegExp(`\\s${name}="([^"]*)"`).exec(tag);
  return m ? m[1] : null;
}

/** The source viewBox, or null when it has none. */
export function readViewBox(svg) {
  return attr(rootTag(svg), 'viewBox');
}

/** "0 0 w h" from width/height when there is no viewBox; throws when neither exists. */
export function synthesiseViewBox(svg) {
  const tag = rootTag(svg);
  const w = parseFloat(attr(tag, 'width') ?? '');
  const h = parseFloat(attr(tag, 'height') ?? '');
  if (!(w > 0) || !(h > 0)) {
    throw new Error('source has neither a viewBox nor width/height');
  }
  return `0 0 ${fmt(w)} ${fmt(h)}`;
}

// ---------------------------------------------------------------------------
// Cropping

const IDENTITY = [1, 0, 0, 1, 0, 0];

function multiply(m, n) {
  const [a, b, c, d, e, f] = m;
  const [a2, b2, c2, d2, e2, f2] = n;
  return [
    a * a2 + c * b2,
    b * a2 + d * b2,
    a * c2 + c * d2,
    b * c2 + d * d2,
    a * e2 + c * f2 + e,
    b * e2 + d * f2 + f,
  ];
}

/** Parse an SVG transform list into one affine matrix. */
export function parseTransform(text) {
  let m = IDENTITY;
  if (!text) return m;
  const re = /(matrix|translate|scale|rotate|skewX|skewY)\s*\(([^)]*)\)/g;
  let hit;
  while ((hit = re.exec(text))) {
    const v = hit[2]
      .split(/[\s,]+/)
      .filter(Boolean)
      .map(Number);
    let t;
    switch (hit[1]) {
      case 'matrix':
        t = v.length === 6 ? v : IDENTITY;
        break;
      case 'translate':
        t = [1, 0, 0, 1, v[0] ?? 0, v[1] ?? 0];
        break;
      case 'scale':
        t = [v[0] ?? 1, 0, 0, v[1] ?? v[0] ?? 1, 0, 0];
        break;
      case 'rotate': {
        const r = ((v[0] ?? 0) * Math.PI) / 180;
        const cx = v[1] ?? 0;
        const cy = v[2] ?? 0;
        t = multiply(
          multiply(
            [1, 0, 0, 1, cx, cy],
            [Math.cos(r), Math.sin(r), -Math.sin(r), Math.cos(r), 0, 0],
          ),
          [1, 0, 0, 1, -cx, -cy],
        );
        break;
      }
      case 'skewX':
        t = [1, 0, Math.tan(((v[0] ?? 0) * Math.PI) / 180), 1, 0, 0];
        break;
      default:
        t = [1, Math.tan(((v[0] ?? 0) * Math.PI) / 180), 0, 1, 0, 0];
    }
    m = multiply(m, t);
  }
  return m;
}

const UNSUPPORTED = /<(rect|circle|ellipse|line|polyline|polygon|use)\b/;

/**
 * Bounding box of every drawn <path d>, honouring <g>/<path> transforms and
 * skipping anything inside <defs>/<clipPath>/<mask>. Null when nothing draws.
 */
export function drawingBbox(svg) {
  const bad = UNSUPPORTED.exec(svg);
  if (bad)
    throw new Error(`unsupported element <${bad[1]}> (convert to a path)`);
  const stack = [IDENTITY];
  let hidden = 0;
  let box = null;
  const tags = /<(\/?)([a-zA-Z][\w:-]*)\b([^>]*?)(\/?)>/g;
  let t;
  while ((t = tags.exec(svg))) {
    const [, closing, name, attrs, selfClosing] = t;
    const nonRendering =
      name === 'defs' || name === 'clipPath' || name === 'mask';
    if (closing) {
      if (nonRendering) hidden -= 1;
      if (name === 'g' || name === 'svg' || nonRendering) stack.pop();
      continue;
    }
    const tag = `<${name}${attrs}>`;
    const m = multiply(
      stack[stack.length - 1],
      parseTransform(attr(tag, 'transform')),
    );
    if (name === 'path' && hidden === 0) {
      const d = attr(tag, 'd');
      if (d && d.trim()) {
        const [x0, y0, x1, y1] = svgPathBbox(d);
        for (const [px, py] of [
          [x0, y0],
          [x1, y0],
          [x0, y1],
          [x1, y1],
        ]) {
          const x = m[0] * px + m[2] * py + m[4];
          const y = m[1] * px + m[3] * py + m[5];
          box = box
            ? [
                Math.min(box[0], x),
                Math.min(box[1], y),
                Math.max(box[2], x),
                Math.max(box[3], y),
              ]
            : [x, y, x, y];
        }
      }
    }
    if (!selfClosing && (name === 'g' || name === 'svg' || nonRendering)) {
      stack.push(m);
      if (nonRendering) hidden += 1;
    }
  }
  return box;
}

function fmt(n) {
  return String(Math.round(n * 1000) / 1000);
}

/** Union bbox of every <path d> (svg-path-bbox), padded 2% per axis, clamped to the source viewBox. */
export function cropViewBox(svg, sourceViewBox) {
  const [sx, sy, sw, sh] = sourceViewBox.split(/[\s,]+/).map(Number);
  const box = drawingBbox(svg);
  if (!box) throw new Error('empty glyph: no drawn path');
  const [x0, y0, x1, y1] = box;
  const padX = (x1 - x0) * 0.02;
  const padY = (y1 - y0) * 0.02;
  const left = Math.max(sx, x0 - padX);
  const top = Math.max(sy, y0 - padY);
  const right = Math.min(sx + sw, x1 + padX);
  const bottom = Math.min(sy + sh, y1 + padY);
  const w = right - left;
  const h = bottom - top;
  if (!(w > 0) || !(h > 0)) {
    throw new Error('empty glyph: drawing falls outside its viewBox');
  }
  return {
    viewBox: `${fmt(left)} ${fmt(top)} ${fmt(w)} ${fmt(h)}`,
    aspect: Math.round((w / h) * 1000) / 1000,
  };
}

// ---------------------------------------------------------------------------
// Checks

const DIRTY = [
  [/#fff(fff)?\b/i, '#fff'],
  [/<text\b/, '<text'],
  [/<image\b/, '<image'],
  [/\bstyle=/, 'style='],
  [/<style\b/, '<style'],
  [/<script\b/, '<script'],
  [/\son[a-z]+=/i, 'event handler'],
  [/\b(?:xlink:)?href=/, 'href'],
  [/<foreignObject\b/, '<foreignObject'],
  [/<(?:animate\w*|set)\b/, 'animation'],
];

/** Throws naming the offending token: #fff, #ffffff, <text, <image, style=, fill="<other>". */
export function assertClean(id, body) {
  for (const [re, label] of DIRTY) {
    if (re.test(body)) throw new Error(`${id}: output still contains ${label}`);
  }
  for (const m of body.matchAll(PAINT)) {
    if (m[2] !== 'currentColor' && m[2] !== 'none') {
      throw new Error(`${id}: ${m[1]}="${m[2]}" is not currentColor or none`);
    }
  }
}

/** Inner markup of an <svg> document. */
export function innerMarkup(svg) {
  const open = rootTag(svg);
  const start = svg.indexOf(open) + open.length;
  const end = svg.lastIndexOf('</svg>');
  if (end < start) return '';
  return svg.slice(start, end).trim();
}

// ---------------------------------------------------------------------------
// Outputs

function sortedIds(record) {
  return Object.keys(record).sort();
}

/** One reviewable SVG file. */
export function toSvgFile(icon) {
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="${icon.viewBox}" fill="currentColor">${icon.body}</svg>\n`;
}

/** The generated TS module text (Prettier runs on it afterwards). */
export function toGenerated(icons) {
  const lines = sortedIds(icons).map((id) => {
    const { viewBox, body, aspect } = icons[id];
    return `  ${JSON.stringify(id)}: { viewBox: ${JSON.stringify(viewBox)}, aspect: ${aspect}, body: ${JSON.stringify(body)} },`;
  });
  return [
    '// Generated by pnpm system-icons. Do not edit.',
    `// RetroArch Assets, XMB Monochrome theme, (c) libretro contributors, CC BY 4.0.`,
    `// ${REPO_URL}/tree/${COMMIT}/${SOURCE_DIR}`,
    '// Changes: recoloured to currentColor, optimised with SVGO, viewBox cropped.',
    '',
    'export interface SystemIconData {',
    '  readonly viewBox: string;',
    '  readonly body: string;',
    '  /** Width / height of the cropped viewBox. */',
    '  readonly aspect: number;',
    '}',
    '',
    'export const SYSTEM_ICONS: Readonly<Record<string, SystemIconData>> = {',
    ...lines,
    '} as const;',
    '',
  ].join('\n');
}

function escapeHtml(s) {
  return s.replace(
    /[&<>"]/g,
    (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c],
  );
}

const SHEET_SIZES = [14, 16, 20, 48];

/** Static HTML: every icon at 14/16/20/48 px in a 1.5x box, both themes. */
export function toContactSheet(icons) {
  const rows = sortedIds(icons)
    .map((id) => {
      const { viewBox, body, aspect } = icons[id];
      const cells = SHEET_SIZES.map((size) => {
        const box = size * 1.5;
        const fit = aspect > 1.5 ? `width="${box}"` : `height="${size}"`;
        return `<td><span class="box" style="width:${box}px;height:${size}px"><svg viewBox="${viewBox}" ${fit} fill="currentColor" aria-hidden="true">${body}</svg></span></td>`;
      }).join('');
      return `<tr data-id="${escapeHtml(id)}"><th scope="row"><code>${escapeHtml(id)}</code></th>${cells}<td class="meta">${aspect}</td></tr>`;
    })
    .join('\n');
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>System icons</title>
<style>
  /* Greyboard roles from src/styles/tokens.css: bg, text, text-secondary, border. */
  :root { color-scheme: light dark; --paper: #f2f3ef; --ink: #1b1e1b; --muted: #535a52; --rule: #d3d7cf; }
  @media (prefers-color-scheme: dark) { :root { --paper: #121514; --ink: #eceeea; --muted: #a8afab; --rule: #2d3331; } }
  body { margin: 0; padding: 24px 16px; background: var(--paper); color: var(--ink); font: 13px/1.4 system-ui, sans-serif; }
  h1 { font-size: 15px; font-weight: 600; margin: 0 0 4px; }
  p { margin: 0 0 16px; color: var(--muted); }
  table { border-collapse: collapse; }
  th, td { padding: 6px 12px; border-bottom: 1px solid var(--rule); text-align: left; vertical-align: middle; }
  thead th { color: var(--muted); font-weight: 500; }
  .box { display: inline-flex; align-items: center; justify-content: center; color: var(--muted); outline: 1px dashed var(--rule); }
  .box svg { display: block; max-width: 100%; max-height: 100%; }
  .meta { color: var(--muted); font-variant-numeric: tabular-nums; }
</style>
</head>
<body>
<h1>System icons</h1>
<p>${Object.keys(icons).length} icons. RetroArch Assets, XMB Monochrome theme, &copy; libretro contributors, CC BY 4.0, commit <code>${COMMIT.slice(0, 12)}</code>. Each icon sits in a box 1.5&times; its height.</p>
<table>
<thead><tr><th>id</th>${SHEET_SIZES.map((s) => `<th>${s}px</th>`).join('')}<th>aspect</th></tr></thead>
<tbody>
${rows}
</tbody>
</table>
</body>
</html>
`;
}

/** README with source, commit, licence, changes and the per-icon audit table. */
export function toReadme(sources) {
  const rows = sortedIds(sources)
    .map((id) => {
      const s = sources[id];
      return `| \`${id}\` | ${s.file.replace(/\|/g, '\\|')} | ${s.depicts} | ${s.audit} |`;
    })
    .join('\n');
  return `# System icons

Generated by \`pnpm system-icons\` (\`scripts/icons/systems.mjs\`). Do not edit
these files by hand; change the source map in
\`scripts/icons/systemIconsLib.mjs\` and run the script again.

## Source

- **Work:** RetroArch Assets, XMB "Monochrome" theme (\`${SOURCE_DIR}\`)
- **Author:** © libretro contributors
- **Repository:** ${REPO_URL}
- **Commit:** [\`${COMMIT}\`](${REPO_URL}/tree/${COMMIT}/${SOURCE_DIR})
- **Licence:** [Creative Commons Attribution 4.0 International (CC BY 4.0)](https://creativecommons.org/licenses/by/4.0/)

## Changes made

- Recoloured: every fill and stroke other than \`none\` is \`currentColor\`.
- Optimised with SVGO (metadata, editor data, ids, classes and styles removed;
  path data re-encoded with coordinates rounded to 3 decimals, which leaves the
  drawn shape visually unchanged).
- Full-canvas clip paths removed; the cropped viewBox does the same job.
- ViewBoxes synthesised from \`width\`/\`height\` where the source had none,
  then cropped to the drawing plus 2% on each axis.

## Trademarks

These are simplified silhouettes of compatible hardware, shown beside the
system's name only to identify it. They imply no endorsement. Icons in the
theme that are a maker's logo or wordmark (CP System, MAME, DOS, ScummVM,
RPG Maker) are not vendored; those systems use the abbreviation fallback.
The CC licence grants no trademark rights. See \`TRADEMARKS.md\`.

## Audit

${Object.keys(sources).length} system ids, each checked against its source
file for maker marks (logos, wordmarks, badges, registered button symbols).

| id | source file | depicts | audit |
|---|---|---|---|
${rows}

The contact sheet (\`contact-sheet.html\`) shows every icon at 14, 16, 20
and 48 px in both themes.
`;
}
