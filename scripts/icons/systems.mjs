#!/usr/bin/env node
// pnpm system-icons: fetch the pinned RetroArch Monochrome SVGs once,
// normalise and crop them, and write the committed outputs:
//   src/assets/system-icons/<id>.svg, README.md, contact-sheet.html
//   src/components/system/systemIcons.generated.ts
// The app never fetches icons at run time; the build needs no network.
// Exits non-zero on any failed check.

import { mkdir, readdir, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import * as prettier from 'prettier';
import {
  SOURCE_BASE,
  SYSTEM_ICON_SOURCES,
  assertClean,
  cropViewBox,
  innerMarkup,
  normaliseSvg,
  readViewBox,
  synthesiseViewBox,
  toContactSheet,
  toGenerated,
  toReadme,
  toSvgFile,
} from './systemIconsLib.mjs';

const ROOT = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  '../..',
);
const ASSETS = path.join(ROOT, 'src/assets/system-icons');
const GENERATED = path.join(
  ROOT,
  'src/components/system/systemIcons.generated.ts',
);

async function fetchSource(file) {
  const url = SOURCE_BASE + encodeURIComponent(file);
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${file}: HTTP ${res.status} from ${url}`);
  return res.text();
}

async function main() {
  const files = [
    ...new Set(Object.values(SYSTEM_ICON_SOURCES).map((s) => s.file)),
  ];
  const raw = new Map();
  for (const file of files) raw.set(file, await fetchSource(file));

  const icons = {};
  for (const [id, { file }] of Object.entries(SYSTEM_ICON_SOURCES)) {
    const source = raw.get(file);
    const sourceViewBox = readViewBox(source) ?? synthesiseViewBox(source);
    const withViewBox = readViewBox(source)
      ? source
      : source.replace(/<svg\b/, `<svg viewBox="${sourceViewBox}"`);
    const clean = normaliseSvg(withViewBox);
    const { viewBox, aspect } = cropViewBox(clean, sourceViewBox);
    const body = innerMarkup(clean);
    assertClean(id, body);
    icons[id] = { viewBox, body, aspect };
  }

  await mkdir(ASSETS, { recursive: true });
  for (const name of await readdir(ASSETS)) {
    if (name.endsWith('.svg')) await rm(path.join(ASSETS, name));
  }
  for (const [id, icon] of Object.entries(icons)) {
    await writeFile(path.join(ASSETS, `${id}.svg`), toSvgFile(icon));
  }
  await writeFile(
    path.join(ASSETS, 'contact-sheet.html'),
    toContactSheet(icons),
  );

  // Prettier-format the files that `pnpm format:check` covers.
  const pretty = async (filepath, text) => {
    const options = (await prettier.resolveConfig(filepath)) ?? {};
    return prettier.format(text, { ...options, filepath });
  };
  const readme = path.join(ASSETS, 'README.md');
  await writeFile(readme, await pretty(readme, toReadme(SYSTEM_ICON_SOURCES)));
  await mkdir(path.dirname(GENERATED), { recursive: true });
  await writeFile(GENERATED, await pretty(GENERATED, toGenerated(icons)));

  console.log(
    `system-icons: ${Object.keys(icons).length} icons from ${files.length} source files`,
  );
}

main().catch((err) => {
  console.error(`system-icons: ${err instanceof Error ? err.message : err}`);
  process.exit(1);
});
