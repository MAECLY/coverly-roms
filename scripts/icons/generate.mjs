#!/usr/bin/env node
/**
 * Rasterise the Coverly ROMs icon from its master SVGs and rebuild every size
 * the app and the landing ask for.
 *
 * Single source: design/icon/ (see design/icon/README.md).
 *   app.svg          1024 squircle app icon (Tauri source, Linux, macOS legacy)
 *   glyph-16/24/32   hand-tuned small sizes (ico layers, favicons)
 *   maskable.svg     full-bleed, glyph inside the 40% safe zone (PWA maskable)
 *
 * Output:
 *   src-tauri/icons/source.png       1024 px, fed to `pnpm tauri icon`, which
 *                                    writes the icns/ico/png/android/ios sets
 *   src-tauri/icons/icon.ico         then rewritten: 32, 16, 24, 48, 64, 256
 *                                    (16/24/32 from the hand-tuned glyphs)
 *   src-tauri/icons/32x32.png        then rewritten from glyph-32.svg
 *   public/favicon.png               32 px, app dev favicon (glyph-32.svg)
 *   site/favicon.ico                 16, 32, 48
 *   site/assets/favicon.svg          16 px glyph with a prefers-color-scheme switch
 *   site/assets/apple-touch-icon.png 180 px, opaque, full bleed
 *   site/assets/icon-192.png, icon-512.png, icon-maskable-512.png
 *
 * Rasteriser: @resvg/resvg-js (devDependency, pinned). If it is missing, the
 * script falls back to `rsvg-convert` or `inkscape` on PATH, and stops with an
 * error naming all three if none is available.
 *
 * Run: pnpm icons   (or: make icons, node scripts/icons/generate.mjs)
 */
import { execFileSync } from 'node:child_process';
import {
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { deflateSync } from 'node:zlib';

const REPO = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const SRC = join(REPO, 'design', 'icon');
const ICONS = join(REPO, 'src-tauri', 'icons');
const SITE = join(REPO, 'site');
const PUBLIC = join(REPO, 'public');

const svg = (name) => readFileSync(join(SRC, name), 'utf8');
const APP = svg('app.svg');
const MASKABLE = svg('maskable.svg');
const GLYPH = {
  16: svg('glyph-16.svg'),
  24: svg('glyph-24.svg'),
  32: svg('glyph-32.svg'),
};

/** app.svg cropped to its 824 px body, tile corners squared: opaque full bleed. */
function fullBleed(appSvg) {
  const tile = /<rect id="tile"[^>]*\/>/;
  if (!tile.test(appSvg))
    throw new Error('app.svg: expected a <rect id="tile" .../>');
  return appSvg
    .replace('viewBox="0 0 1024 1024"', 'viewBox="100 100 824 824"')
    .replace(
      tile,
      '<rect id="tile" x="100" y="100" width="824" height="824" fill="#0E5E6F"/>',
    );
}

/** app.svg cropped to its body, tile corners kept: for ico layers (all tiles full size). */
function tileOnly(appSvg) {
  return appSvg.replace('viewBox="0 0 1024 1024"', 'viewBox="100 100 824 824"');
}

// ---------------------------------------------------------------- rasteriser

let resvg = null;
try {
  resvg = await import('@resvg/resvg-js');
} catch {
  resvg = null;
}

function onPath(cmd) {
  try {
    execFileSync(process.platform === 'win32' ? 'where' : 'which', [cmd], {
      stdio: 'ignore',
    });
    return true;
  } catch {
    return false;
  }
}

const CLI = resvg ? null : ['rsvg-convert', 'inkscape'].find(onPath);
if (!resvg && !CLI) {
  console.error(
    'No SVG rasteriser found. Install one of:\n' +
      '  pnpm install            (@resvg/resvg-js, the default devDependency)\n' +
      '  rsvg-convert            (brew install librsvg / apt install librsvg2-bin)\n' +
      '  inkscape                (https://inkscape.org)',
  );
  process.exit(1);
}

/** Render an SVG string to a PNG buffer of size x size. */
function rasterise(source, size, { opaque = false } = {}) {
  if (resvg) {
    const img = new resvg.Resvg(source, {
      fitTo: { mode: 'width', value: size },
    }).render();
    if (img.width !== size || img.height !== size) {
      throw new Error(
        `expected ${size}x${size}, got ${img.width}x${img.height}`,
      );
    }
    return encodePng(size, size, img.pixels, opaque);
  }
  const dir = mkdtempSync(join(tmpdir(), 'coverly-icon-'));
  try {
    const input = join(dir, 'in.svg');
    const output = join(dir, 'out.png');
    writeFileSync(input, source);
    const s = String(size);
    if (CLI === 'rsvg-convert') {
      execFileSync('rsvg-convert', ['-w', s, '-h', s, '-o', output, input]);
    } else {
      execFileSync('inkscape', [
        input,
        '--export-type=png',
        `--export-filename=${output}`,
        '-w',
        s,
        '-h',
        s,
      ]);
    }
    return readFileSync(output);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

// ------------------------------------------------------------------ encoders

const CRC_TABLE = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});

function crc32(buf) {
  let c = 0xffffffff;
  for (const b of buf) c = CRC_TABLE[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, 'ascii'), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([len, body, crc]);
}

/** RGBA pixels to PNG; `opaque` drops the alpha channel (apple-touch-icon). */
function encodePng(width, height, rgba, opaque) {
  const ch = opaque ? 3 : 4;
  const raw = Buffer.alloc(height * (width * ch + 1));
  let o = 0;
  for (let y = 0; y < height; y++) {
    raw[o++] = 0; // filter: none
    for (let x = 0; x < width; x++) {
      const i = (y * width + x) * 4;
      if (opaque && rgba[i + 3] !== 255)
        throw new Error('opaque PNG requested from a transparent render');
      for (let c = 0; c < ch; c++) raw[o++] = rgba[i + c];
    }
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = opaque ? 2 : 6; // RGB or RGBA
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', ihdr),
    chunk('IDAT', deflateSync(raw, { level: 9 })),
    chunk('IEND', Buffer.alloc(0)),
  ]);
}

/** ICO with PNG-compressed layers, written in the given order. */
function encodeIco(layers) {
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0);
  header.writeUInt16LE(1, 2); // type: icon
  header.writeUInt16LE(layers.length, 4);
  const dir = Buffer.alloc(16 * layers.length);
  let offset = 6 + dir.length;
  layers.forEach(({ size, png }, i) => {
    const e = i * 16;
    dir[e] = size >= 256 ? 0 : size;
    dir[e + 1] = size >= 256 ? 0 : size;
    dir.writeUInt16LE(1, e + 4); // planes
    dir.writeUInt16LE(32, e + 6); // bits per pixel
    dir.writeUInt32LE(png.length, e + 8);
    dir.writeUInt32LE(offset, e + 12);
    offset += png.length;
  });
  return Buffer.concat([header, dir, ...layers.map((l) => l.png)]);
}

/** Small sizes use the hand-tuned glyphs, larger ones the app tile. */
function icoLayer(size) {
  return { size, png: rasterise(GLYPH[size] ?? tileOnly(APP), size) };
}

// ------------------------------------------------------------ favicon.svg

/** The 16 px glyph with a dark-scheme palette (tile stays Buckram, L ~0.47). */
function faviconSvg() {
  const classes = { '#0E5E6F': 't', '#FBFBF9': 'p', '#79A5AD': 'l' };
  let out = GLYPH[16].replace(/<title>[^<]*<\/title>\n?/, '');
  for (const [hex, cls] of Object.entries(classes)) {
    if (!out.includes(`fill="${hex}"`))
      throw new Error(`glyph-16.svg: missing fill ${hex}`);
    out = out.replaceAll(`fill="${hex}"`, `class="${cls}"`);
  }
  const style =
    '<style>.t{fill:#0E5E6F}.p{fill:#FBFBF9}.l{fill:#79A5AD}' +
    '@media (prefers-color-scheme:dark){.t{fill:#205F6A}.p{fill:#ECEEEA}.l{fill:#7C9FA4}}</style>';
  return out.replace(
    /(<svg[^>]*>)/,
    `$1\n<!-- Generated by scripts/icons/generate.mjs from design/icon/glyph-16.svg. Do not edit. -->\n${style}`,
  );
}

// --------------------------------------------------------------------- build

function write(path, data) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, data);
  console.log('  wrote', path.slice(REPO.length + 1));
}

console.log(`rasterising with ${resvg ? '@resvg/resvg-js' : CLI}`);

// Desktop app: Tauri derives every platform set from the 1024 source.
const SOURCE = join(ICONS, 'source.png');
write(SOURCE, rasterise(APP, 1024));
execFileSync('pnpm', ['tauri', 'icon', SOURCE, '--output', ICONS], {
  cwd: REPO,
  stdio: 'inherit',
  shell: process.platform === 'win32',
});
// Then replace the downscaled small sizes with the hand-tuned glyphs.
write(
  join(ICONS, 'icon.ico'),
  encodeIco([32, 16, 24, 48, 64, 256].map(icoLayer)),
);
write(join(ICONS, '32x32.png'), rasterise(GLYPH[32], 32));
write(join(PUBLIC, 'favicon.png'), rasterise(GLYPH[32], 32));

// Landing.
write(join(SITE, 'favicon.ico'), encodeIco([16, 32, 48].map(icoLayer)));
write(join(SITE, 'assets', 'favicon.svg'), faviconSvg());
write(
  join(SITE, 'assets', 'apple-touch-icon.png'),
  rasterise(fullBleed(APP), 180, { opaque: true }),
);
write(join(SITE, 'assets', 'icon-192.png'), rasterise(APP, 192));
write(join(SITE, 'assets', 'icon-512.png'), rasterise(APP, 512));
write(
  join(SITE, 'assets', 'icon-maskable-512.png'),
  rasterise(MASKABLE, 512, { opaque: true }),
);

console.log('icons regenerated from design/icon/');
