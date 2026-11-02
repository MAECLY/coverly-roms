#!/usr/bin/env node
/**
 * README credits block (adjustments D25).
 *
 *   pnpm credits:readme   rewrite the block in README.md (en) and README.es.md (es)
 *   pnpm credits:check    exit 1 when a README's block differs from credits.json
 *
 * The block sits between `<!-- credits:start -->` and `<!-- credits:end -->`.
 * Entries come from `credits/credits.json`; the group headings, the intro and
 * the «no content» line come from the settings tab's strings in `locales/`,
 * so the README says exactly what the «Agradecimientos» tab says.
 */
import { readFileSync, realpathSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const REPO = join(dirname(fileURLToPath(import.meta.url)), '..', '..');

export const START = '<!-- credits:start -->';
export const END = '<!-- credits:end -->';

/** Each README and the language its block is written in. */
export const READMES = [
  { file: 'README.md', locale: 'en' },
  { file: 'README.es.md', locale: 'es' },
];

export const GROUPS = [
  'art-sources',
  'databases',
  'docs',
  'conventions',
  'libraries',
  'fonts-icons',
];

const LABELS = {
  en: {
    generated:
      'Generated from credits/credits.json by `pnpm credits:readme`; do not edit by hand.',
    licence: 'Licence',
    notices:
      'The licences of every Rust crate and npm package in the build are in `LICENSES/generated/`, and the app shows them in Settings → Credits.',
  },
  es: {
    generated:
      'Generado desde credits/credits.json con `pnpm credits:readme`; no lo edites a mano.',
    licence: 'Licencia',
    notices:
      'Las licencias de cada crate de Rust y paquete de npm de la compilación están en `LICENSES/generated/`, y la app las muestra en Ajustes → Agradecimientos.',
  },
};

function pick(label, locale) {
  return typeof label === 'string' ? label : label[locale];
}

/** Square brackets would break the link syntax around a name. */
function escape(text) {
  return text.replace(/([[\]])/g, '\\$1');
}

function link(text, url) {
  return url ? `[${escape(text)}](${url})` : escape(text);
}

function readJson(root, ...parts) {
  return JSON.parse(readFileSync(join(root, ...parts), 'utf8'));
}

/** The Markdown that goes between the markers, for one language. */
export function renderBlock(root, locale) {
  const { entries } = readJson(root, 'credits', 'credits.json');
  const strings = readJson(root, 'locales', locale, 'ui.json').settings.credits;
  const labels = LABELS[locale];
  const out = [`<!-- ${labels.generated} -->`, '', strings.intro, ''];
  for (const group of GROUPS) {
    const inGroup = entries.filter((e) => e.group === group);
    if (inGroup.length === 0) continue;
    out.push(`### ${strings.groups[group]}`, '');
    for (const e of inGroup) {
      const licence = link(pick(e.license.name, locale), e.license.url);
      out.push(
        `- ${link(pick(e.name, locale), e.url)}. ${e.use[locale]} ${labels.licence}: ${licence}.`,
      );
      if (e.credit) out.push('', `  > ${e.credit[locale]}`, '');
    }
    out.push('');
  }
  out.push(strings.noContent, labels.notices);
  // Collapse the blank lines a trailing quote leaves before the next item.
  return out.join('\n').replace(/\n{3,}/g, '\n\n');
}

/** `text` with the block between the markers replaced by `block`. */
export function replaceBlock(text, block) {
  const start = text.indexOf(START);
  const end = text.indexOf(END);
  if (start < 0 || end < start) {
    throw new Error(`missing ${START} … ${END} markers`);
  }
  return `${text.slice(0, start + START.length)}\n${block}\n${text.slice(end)}`;
}

/** The README files whose block is out of date (empty when all match). */
export function checkReadmes(root) {
  const stale = [];
  for (const { file, locale } of READMES) {
    const path = join(root, file);
    const text = readFileSync(path, 'utf8').replace(/\r\n/g, '\n');
    let next;
    try {
      next = replaceBlock(text, renderBlock(root, locale));
    } catch (err) {
      stale.push(`${file}: ${err.message}`);
      continue;
    }
    if (next !== text) stale.push(`${file} is out of date`);
  }
  return stale;
}

/** Rewrites the block in every README. */
export function writeReadmes(root) {
  for (const { file, locale } of READMES) {
    const path = join(root, file);
    const text = readFileSync(path, 'utf8').replace(/\r\n/g, '\n');
    writeFileSync(path, replaceBlock(text, renderBlock(root, locale)));
  }
}

if (process.argv[1] && realpathSync(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (process.argv.includes('--check')) {
    const stale = checkReadmes(REPO);
    if (stale.length > 0) {
      for (const s of stale) console.error(`credits: ${s}`);
      console.error('credits: run pnpm credits:readme');
      process.exit(1);
    }
    console.log('credits: README blocks match credits.json');
  } else {
    writeReadmes(REPO);
    console.log('credits: README blocks written');
  }
}
