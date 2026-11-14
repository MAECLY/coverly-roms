#!/usr/bin/env node
/**
 * Build check for the PDF viewer (extras spec D18), run after `vite build`:
 *
 * - `dist/pdfjs/cmaps/` and `dist/pdfjs/standard_fonts/` hold pdf.js's data
 *   (with their licence files);
 * - pdf.js is not in the entry chunk: it loads only when a PDF opens;
 * - its own chunk and its worker are there, and neither contains an eval
 *   path (`eval(`, `new Function(`), so the CSP never needs 'unsafe-eval';
 * - nothing ships a `.wasm` file (the CSP has no 'wasm-unsafe-eval').
 *
 *   node scripts/build/check-pdfjs.mjs [distDir]
 */
import { existsSync, readFileSync, readdirSync, realpathSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const REPO = join(dirname(fileURLToPath(import.meta.url)), '..', '..');

/** pdf.js's version string, which its main module carries verbatim. */
function pdfjsVersion(root) {
  const pkg = join(root, 'node_modules', 'pdfjs-dist', 'package.json');
  return JSON.parse(readFileSync(pkg, 'utf8')).version;
}

const EVAL = /(?<![\w$.])eval\s*\(|new\s+Function\s*\(/;

/** Every problem found in `dist`; empty when the build is right. */
export function checkPdfjsBuild(dist, version) {
  const problems = [];
  for (const dir of ['cmaps', 'standard_fonts']) {
    const at = join(dist, 'pdfjs', dir);
    if (!existsSync(at) || readdirSync(at).length === 0) {
      problems.push(`dist/pdfjs/${dir} is missing or empty`);
    }
  }
  if (!existsSync(join(dist, 'pdfjs', 'cmaps', 'LICENSE'))) {
    problems.push('dist/pdfjs/cmaps/LICENSE is missing');
  }

  const html = readFileSync(join(dist, 'index.html'), 'utf8');
  const entry = /<script[^>]+type="module"[^>]+src="\/?([^"]+)"/.exec(html)?.[1];
  if (!entry) {
    problems.push('no module entry script in dist/index.html');
    return problems;
  }
  const entryCode = readFileSync(join(dist, entry), 'utf8');
  if (entryCode.includes(`"${version}"`) || entryCode.includes(`'${version}'`)) {
    problems.push(`the entry chunk ${entry} contains pdf.js ${version}`);
  }

  const assets = readdirSync(join(dist, 'assets'));
  const chunk = assets.find((f) => /^pdfjs-.*\.js$/.test(f));
  const worker = assets.find((f) => /^pdf\.worker.*\.m?js$/.test(f));
  if (!chunk) problems.push('no pdfjs-*.js chunk in dist/assets');
  if (!worker) problems.push('no pdf.worker asset in dist/assets');
  for (const f of [chunk, worker].filter(Boolean)) {
    if (EVAL.test(readFileSync(join(dist, 'assets', f), 'utf8'))) {
      problems.push(`dist/assets/${f} contains an eval path`);
    }
  }
  if (chunk && !readFileSync(join(dist, 'assets', chunk), 'utf8').includes(version)) {
    problems.push(`dist/assets/${chunk} is not pdf.js ${version}`);
  }
  if (assets.some((f) => f.endsWith('.wasm'))) {
    problems.push('dist/assets ships a .wasm file');
  }
  return problems;
}

if (process.argv[1] && realpathSync(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const dist = process.argv[2] ?? join(REPO, 'dist');
  const problems = checkPdfjsBuild(dist, pdfjsVersion(REPO));
  if (problems.length > 0) {
    for (const p of problems) console.error(`pdfjs: ${p}`);
    process.exit(1);
  }
  console.log('pdfjs: data bundled, own chunk, worker same-origin, no eval');
}
