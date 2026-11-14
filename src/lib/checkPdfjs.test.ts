import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';

import {
  checkPdfjsBuild,
  staticImports,
} from '../../scripts/build/check-pdfjs.mjs';

const VERSION = '6.4.299';
const dirs: string[] = [];

afterEach(() => {
  for (const d of dirs.splice(0)) rmSync(d, { recursive: true, force: true });
});

/** A `dist/` shaped like Vite's, with the given extra html and chunks. */
function dist({
  preload = '',
  entry = 'import{t}from"./rolldown-runtime-a.js";import("./pdfjs-b.js");',
  extra = {},
}: {
  preload?: string;
  entry?: string;
  extra?: Record<string, string>;
} = {}): string {
  const d = mkdtempSync(join(tmpdir(), 'coverly-pdfjs-'));
  dirs.push(d);
  for (const sub of ['cmaps', 'standard_fonts']) {
    mkdirSync(join(d, 'pdfjs', sub), { recursive: true });
    writeFileSync(join(d, 'pdfjs', sub, 'LICENSE'), 'licence');
  }
  mkdirSync(join(d, 'assets'));
  writeFileSync(
    join(d, 'index.html'),
    `<script type="module" crossorigin src="/assets/index-a.js"></script>\n${preload}`,
  );
  const files: Record<string, string> = {
    'index-a.js': entry,
    'rolldown-runtime-a.js': 'export const t=1;',
    'pdfjs-b.js': `import{t}from"./rolldown-runtime-a.js";const v="${VERSION}";`,
    'pdf.worker.min-c.mjs': 'self.x=1;',
    ...extra,
  };
  for (const [name, code] of Object.entries(files)) {
    writeFileSync(join(d, 'assets', name), code);
  }
  return d;
}

describe('checkPdfjsBuild', () => {
  it('passes a build where pdf.js is only imported dynamically', () => {
    expect(checkPdfjsBuild(dist(), VERSION)).toEqual([]);
  });

  it('fails when the entry imports the pdf.js chunk statically', () => {
    const d = dist({
      entry: 'import{a as n}from"./pdfjs-b.js";console.log(n);',
    });
    expect(checkPdfjsBuild(d, VERSION)).toContainEqual(
      expect.stringContaining('loads pdfjs-b.js at startup'),
    );
  });

  it('fails when a chunk the entry imports pulls pdf.js in', () => {
    const d = dist({
      entry: 'import"./shared-d.js";',
      extra: { 'shared-d.js': 'export*from"./pdfjs-b.js";' },
    });
    expect(checkPdfjsBuild(d, VERSION)).toContainEqual(
      expect.stringContaining('loads pdfjs-b.js at startup'),
    );
  });

  it('fails when index.html preloads the pdf.js chunk', () => {
    const d = dist({
      preload:
        '<link rel="modulepreload" crossorigin href="/assets/pdfjs-b.js">',
    });
    expect(checkPdfjsBuild(d, VERSION)).toContainEqual(
      expect.stringContaining('preloads assets/pdfjs-b.js'),
    );
  });
});

describe('staticImports', () => {
  it('follows static imports and re-exports, not dynamic ones', () => {
    const d = dist({
      entry:
        'import{t}from"./rolldown-runtime-a.js";import"./shared-d.js";import("./pdfjs-b.js");',
      extra: { 'shared-d.js': 'export{t}from"./rolldown-runtime-a.js";' },
    });
    expect(staticImports(d, 'assets/index-a.js').sort()).toEqual([
      'index-a.js',
      'rolldown-runtime-a.js',
      'shared-d.js',
    ]);
  });
});
