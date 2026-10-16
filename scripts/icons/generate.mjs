#!/usr/bin/env node
/**
 * Draw the placeholder app icon and rebuild every size each OS asks for.
 *
 * The Coverly mark is still being designed (TRADEMARKS.md). Until it ships,
 * the icon is a plain tile with a "C" monogram, drawn here without any image
 * library: the 1024 px PNG is rasterised in pure JS (4x4 supersampling) and
 * encoded with node:zlib, then `pnpm tauri icon` derives every platform size.
 *
 * Run: pnpm icons   (node scripts/icons/generate.mjs)
 */
import { execFileSync } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { deflateSync } from 'node:zlib';

const REPO = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const ICONS = join(REPO, 'src-tauri', 'icons');
const SOURCE = join(ICONS, 'source.png');

const SIZE = 1024;
const SS = 4; // supersamples per axis

// Palette (src/styles/globals.css: --cr-accent light/dark).
const TILE_TOP = [0x63, 0x66, 0xf1];
const TILE_BOTTOM = [0x43, 0x38, 0xca];
const MARK = [0xff, 0xff, 0xff];

// Geometry in 0..1 canvas units. Apple's grid: tile body is 824 of 1024 px.
const INSET = 100 / 1024;
const RADIUS = 0.185;
const RING_OUTER = 0.3;
const RING_INNER = 0.19;
const GAP_HALF_ANGLE = (38 * Math.PI) / 180; // opening of the "C", to the right

function insideTile(x, y) {
  const lo = INSET;
  const hi = 1 - INSET;
  if (x < lo || x > hi || y < lo || y > hi) return false;
  const cx = Math.min(Math.max(x, lo + RADIUS), hi - RADIUS);
  const cy = Math.min(Math.max(y, lo + RADIUS), hi - RADIUS);
  return (x - cx) ** 2 + (y - cy) ** 2 <= RADIUS ** 2;
}

function insideMark(x, y) {
  const dx = x - 0.5;
  const dy = y - 0.5;
  const r = Math.hypot(dx, dy);
  if (r < RING_INNER || r > RING_OUTER) return false;
  return Math.abs(Math.atan2(dy, dx)) > GAP_HALF_ANGLE;
}

function pixel(px, py) {
  let tile = 0;
  let mark = 0;
  for (let sy = 0; sy < SS; sy++) {
    for (let sx = 0; sx < SS; sx++) {
      const x = (px + (sx + 0.5) / SS) / SIZE;
      const y = (py + (sy + 0.5) / SS) / SIZE;
      if (insideTile(x, y)) {
        tile++;
        if (insideMark(x, y)) mark++;
      }
    }
  }
  const n = SS * SS;
  const t = py / (SIZE - 1);
  const base = TILE_TOP.map((c, i) => c + (TILE_BOTTOM[i] - c) * t);
  const m = tile === 0 ? 0 : mark / tile;
  const rgb = base.map((c, i) => Math.round(c + (MARK[i] - c) * m));
  return [...rgb, Math.round((tile / n) * 255)];
}

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

function encodePng() {
  const raw = Buffer.alloc(SIZE * (SIZE * 4 + 1));
  let o = 0;
  for (let y = 0; y < SIZE; y++) {
    raw[o++] = 0; // filter: none
    for (let x = 0; x < SIZE; x++) {
      for (const v of pixel(x, y)) raw[o++] = v;
    }
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(SIZE, 0);
  ihdr.writeUInt32BE(SIZE, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // RGBA
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', ihdr),
    chunk('IDAT', deflateSync(raw, { level: 9 })),
    chunk('IEND', Buffer.alloc(0)),
  ]);
}

mkdirSync(ICONS, { recursive: true });
writeFileSync(SOURCE, encodePng());
execFileSync('pnpm', ['tauri', 'icon', SOURCE, '--output', ICONS], {
  cwd: REPO,
  stdio: 'inherit',
  shell: process.platform === 'win32',
});
console.log('icons regenerated in', ICONS);
