#!/usr/bin/env bash
# Make the media files of the browser mock's Extras library
# (`?mockPick=extras`, src/ipc/mock/mockExtras.ts) so the viewer has
# something of ours to show. No third-party content:
#
#   sample.png   320x180 drawn pixel by pixel below (Node, zlib only)
#   sample.pdf   two pages of plain text, written by hand below (Node)
#   sample.webm  1 s of ffmpeg's own `testsrc` pattern, VP9, no audio
#
# The audio is not a file: src/ipc/mock/media/tone.ts makes a 1 s 440 Hz
# sine WAV in JS when the player asks for it.
#
#   scripts/dev/make-mock-media.sh [DIR]   (default: src/ipc/mock/media)
#
# Needs node and ffmpeg (with libvpx). Re-running overwrites the three files.
set -euo pipefail

dir="${1:-src/ipc/mock/media}"
mkdir -p "$dir"

node - "$dir/sample.png" <<'JS'
// A 320x180 RGB PNG: a dusk gradient, a sun and stepped hills.
const fs = require('node:fs');
const zlib = require('node:zlib');
const [w, h] = [320, 180];
const raw = Buffer.alloc((w * 3 + 1) * h);
for (let y = 0; y < h; y += 1) {
  const row = y * (w * 3 + 1);
  raw[row] = 0; // filter: none
  for (let x = 0; x < w; x += 1) {
    const t = y / h;
    let [r, g, b] = [40 + 150 * t, 50 + 60 * t, 110 - 40 * t];
    const dx = x - 220;
    const dy = y - 80;
    if (dx * dx + dy * dy < 30 * 30) [r, g, b] = [242, 196, 120];
    const hill = 130 + 12 * Math.sin(x / 23) + (x > 160 ? 8 : 0);
    if (y > hill) [r, g, b] = [36, 70, 58];
    if (y > hill + 22) [r, g, b] = [26, 48, 42];
    const i = row + 1 + x * 3;
    raw[i] = Math.round(r);
    raw[i + 1] = Math.round(g);
    raw[i + 2] = Math.round(b);
  }
}
const crcTable = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k += 1) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});
const crc = (buf) => {
  let c = 0xffffffff;
  for (const byte of buf) c = crcTable[(c ^ byte) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
};
const chunk = (type, data) => {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, 'ascii'), data]);
  const sum = Buffer.alloc(4);
  sum.writeUInt32BE(crc(body));
  return Buffer.concat([len, body, sum]);
};
const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(w, 0);
ihdr.writeUInt32BE(h, 4);
ihdr[8] = 8; // bit depth
ihdr[9] = 2; // colour type: RGB
fs.writeFileSync(
  process.argv[2],
  Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', ihdr),
    chunk('IDAT', zlib.deflateSync(raw, { level: 9 })),
    chunk('IEND', Buffer.alloc(0)),
  ]),
);
JS

node - "$dir/sample.pdf" <<'JS'
// A two-page PDF 1.4 with Helvetica text and a correct xref table.
const fs = require('node:fs');
const page = (n) =>
  [
    'BT /F1 24 Tf 72 720 Td (Coverly mock manual) Tj ET',
    `BT /F1 14 Tf 72 680 Td (Page ${n} of 2) Tj ET`,
    'BT /F1 12 Tf 72 640 Td (A sample made for the browser mock.) Tj ET',
  ].join('\n');
const contents = [page(1), page(2)];
const objects = [
  '<< /Type /Catalog /Pages 2 0 R >>',
  '<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>',
  '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 6 0 R >>',
  '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 7 0 R >>',
  '<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>',
  ...contents.map((c) => `<< /Length ${c.length} >>\nstream\n${c}\nendstream`),
];
let out = '%PDF-1.4\n';
const offsets = [];
objects.forEach((body, i) => {
  offsets.push(out.length);
  out += `${i + 1} 0 obj\n${body}\nendobj\n`;
});
const xref = out.length;
out += `xref\n0 ${objects.length + 1}\n0000000000 65535 f \n`;
for (const o of offsets) out += `${String(o).padStart(10, '0')} 00000 n \n`;
out += `trailer\n<< /Size ${objects.length + 1} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`;
fs.writeFileSync(process.argv[2], out, 'latin1');
JS

ffmpeg -hide_banner -loglevel error -y -f lavfi -i testsrc=d=1:s=320x180 \
  -c:v libvpx-vp9 -fflags +bitexact -map_metadata -1 "$dir/sample.webm"

ls -l "$dir"/sample.*
