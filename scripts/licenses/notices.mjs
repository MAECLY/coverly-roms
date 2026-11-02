#!/usr/bin/env node
/**
 * Third-party notices (spec D28).
 *
 *   pnpm notices          regenerate LICENSES/generated/{rust,js}.txt
 *   pnpm notices:check    fail with «run pnpm notices» when a lock changed
 *
 * Generation runs on a developer machine (needs cargo-about 0.9.2); the
 * check is pure Node, so `check-all` and the release jobs never need
 * cargo-about and never regenerate.
 *
 * Both files share one plain-text format, which `src/lib/notices.ts`
 * splits into blocks for the in-app viewer:
 *
 *   lock-sha256: <hex>
 *
 *   ================================================================  (80)
 *   Package: <name> <version>        (one line per package using the text)
 *   License: <SPDX expression>
 *   ----------------------------------------------------------------  (80)
 *   <licence text>
 */
import { execFileSync, spawnSync } from 'node:child_process';
import {
  existsSync,
  readFileSync,
  readdirSync,
  realpathSync,
  writeFileSync,
} from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { LOCK_PREFIX, NOTICE_FILES, checkNotices, expectedHash } from './lock.mjs';

const REPO = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const CARGO_ABOUT = '0.9.2';
export const RULE = '='.repeat(80);
export const SUBRULE = '-'.repeat(80);

/**
 * devDependencies whose code still ends up in the shipped bundle. The
 * minified build keeps no banner comments, so the list comes from the code
 * found in `dist/assets` after `pnpm build`: Tailwind's preflight (with its
 * bundled modern-normalize) and the `tailwindcss-animate` keyframes in the
 * CSS, Vite's module-preload helper (`vite:preloadError`) and Rolldown's
 * CommonJS interop runtime (React ships as CommonJS) in the JS.
 * `via` names the direct devDependency whose pnpm folder holds a transitive
 * one.
 */
export const BUNDLED_DEV = [
  { name: 'tailwindcss' },
  { name: 'tailwindcss-animate' },
  { name: 'vite' },
  { name: 'rolldown', via: 'vite' },
];

const LICENCE_FILE = /^(licen[cs]e|copying|notice)([._-].*)?$/i;

/** One block of the shared format. */
export function block(packages, license, text) {
  return [
    RULE,
    ...packages.map((p) => `Package: ${p}`),
    `License: ${license}`,
    SUBRULE,
    text.replace(/\r\n?/g, '\n').trim(),
    '',
  ].join('\n');
}

function withHeader(hash, blocks) {
  return `${LOCK_PREFIX}${hash}\n\n${blocks.join('\n')}`;
}

/** The workspace's own crate names (Cargo.lock packages without a source). */
function workspaceCrates(cargoLock) {
  const names = new Set();
  for (const part of cargoLock.replace(/\r\n?/g, '\n').split(/^\[\[package\]\]\n/m)) {
    const name = /^name = "([^"]+)"/m.exec(part)?.[1];
    if (name && !/^source = /m.test(part)) names.add(name);
  }
  return names;
}

/**
 * cargo-about output (rendered through about.hbs) → blocks without the
 * workspace's own crates and with each package listed once per block.
 */
export function rustBlocks(rendered, ownCrates) {
  const out = [];
  for (const chunk of rendered.replace(/\r\n?/g, '\n').split(`${RULE}\n`).slice(1)) {
    const lines = chunk.split('\n');
    const packages = [];
    let i = 0;
    for (; lines[i]?.startsWith('Package: '); i++) {
      const pkg = lines[i].slice('Package: '.length).trim();
      const name = pkg.split(' ')[0];
      if (!ownCrates.has(name) && !packages.includes(pkg)) packages.push(pkg);
    }
    const license = lines[i]?.startsWith('License: ')
      ? lines[i].slice('License: '.length).trim()
      : '';
    if (lines[i + 1] !== SUBRULE) throw new Error('unexpected about.hbs output');
    if (packages.length === 0) continue;
    out.push(block(packages, license, lines.slice(i + 2).join('\n')));
  }
  return out;
}

function licenceText(dir, fallback) {
  const files = readdirSync(dir)
    .filter((f) => LICENCE_FILE.test(f))
    .sort();
  if (files.length === 0) return `${fallback} (the package ships no licence file)`;
  return files.map((f) => readFileSync(join(dir, f), 'utf8').trim()).join('\n\n');
}

/** `pnpm licenses list --prod --json` + BUNDLED_DEV → one block per package. */
export function jsBlocks(prod, devPackages) {
  const entries = [];
  for (const [license, pkgs] of Object.entries(prod)) {
    for (const p of pkgs) {
      p.versions.forEach((version, i) => {
        entries.push({ name: p.name, version, license, dir: p.paths[i] });
      });
    }
  }
  entries.push(...devPackages);
  entries.sort((a, b) =>
    a.name === b.name
      ? a.version.localeCompare(b.version)
      : a.name < b.name
        ? -1
        : 1,
  );
  return entries.map((e) =>
    block([`${e.name} ${e.version}`], e.license, licenceText(e.dir, e.license)),
  );
}

function devPackage({ name, via }) {
  const dir = via
    ? join(dirname(realpathSync(join(REPO, 'node_modules', via))), name)
    : realpathSync(join(REPO, 'node_modules', name));
  if (!existsSync(join(dir, 'package.json'))) {
    throw new Error(`notices: ${name} not found in node_modules (pnpm install)`);
  }
  const pkg = JSON.parse(readFileSync(join(dir, 'package.json'), 'utf8'));
  return { name, version: pkg.version, license: pkg.license ?? 'UNKNOWN', dir };
}

function generate() {
  const probe = spawnSync('cargo', ['about', '--version'], { encoding: 'utf8' });
  if (probe.status !== 0) {
    console.error(
      `cargo-about is not installed; install with: cargo install cargo-about --locked --version ${CARGO_ABOUT}`,
    );
    process.exit(1);
  }
  const [rust, js] = NOTICE_FILES;
  const rendered = execFileSync(
    'cargo',
    ['about', 'generate', '--locked', 'about.hbs'],
    { cwd: REPO, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 },
  );
  const own = workspaceCrates(readFileSync(join(REPO, 'Cargo.lock'), 'utf8'));
  const prod = JSON.parse(
    execFileSync('pnpm', ['licenses', 'list', '--prod', '--json'], {
      cwd: REPO,
      encoding: 'utf8',
      maxBuffer: 64 * 1024 * 1024,
    }),
  );
  // Build both before writing either: a failure leaves the files untouched.
  const rustText = withHeader(expectedHash(REPO, rust), rustBlocks(rendered, own));
  const jsText = withHeader(
    expectedHash(REPO, js),
    jsBlocks(prod, BUNDLED_DEV.map(devPackage)),
  );
  writeFileSync(join(REPO, rust.file), rustText);
  writeFileSync(join(REPO, js.file), jsText);
  console.log(`notices: wrote ${rust.file} and ${js.file}`);
}

function check(root) {
  const problems = checkNotices(root);
  if (problems.length > 0) {
    for (const p of problems) console.error(`notices: ${p}`);
    console.error('notices: run pnpm notices');
    process.exit(1);
  }
  console.log('notices: up to date');
}

if (process.argv[1] && realpathSync(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2);
  if (args.includes('--check')) {
    const at = args.indexOf('--root');
    check(at >= 0 && args[at + 1] ? args[at + 1] : REPO);
  } else {
    generate();
  }
}
