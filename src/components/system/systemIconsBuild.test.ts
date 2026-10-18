import { describe, expect, it } from 'vitest';
import {
  assertClean,
  cropViewBox,
  normaliseSvg,
  synthesiseViewBox,
  SYSTEM_ICON_SOURCES,
} from '../../../scripts/icons/systemIconsLib.mjs';

const WHITE =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 128 64 64"><path fill="#ffffff" d="M10 150h44v20H10z"/></svg>';
const STYLED =
  '<svg viewBox="0 0 256 256"><path style="fill:#ffffff" d="M0 0h10v10H0z"/><path style="fill:none" d="M1 1h2v2H1z"/></svg>';
const NO_VIEWBOX =
  '<svg width="64" height="64"><path fill="#fff" d="M8 8h48v48H8z"/></svg>';

describe('normaliseSvg', () => {
  it('turns every non-none fill into currentColor', () => {
    const out = normaliseSvg(WHITE);
    expect(out).toContain('fill="currentColor"');
    expect(out).not.toMatch(/#fff/i);
  });
  it('turns style fills into attributes and keeps fill none', () => {
    const out = normaliseSvg(STYLED);
    expect(out).not.toContain('style=');
    expect(out).toContain('currentColor');
    expect(out).toMatch(/fill="none"/);
  });
});

describe('normaliseSvg clip paths', () => {
  it('drops clip paths and their references', () => {
    const out = normaliseSvg(
      '<svg viewBox="0 0 64 64"><defs><clipPath id="c"><path d="M0 0h64v64H0z"/></clipPath></defs>' +
        '<g clip-path="url(#c)"><path fill="#fff" d="M8 8h48v8H8z"/></g></svg>',
    );
    expect(out).not.toMatch(/clip/i);
    expect(out).not.toMatch(/\sid=/);
    expect(out).toContain('currentColor');
  });
});

describe('synthesiseViewBox', () => {
  it('builds one from width and height', () => {
    expect(synthesiseViewBox(NO_VIEWBOX)).toBe('0 0 64 64');
  });
  it('throws when there is neither', () => {
    expect(() => synthesiseViewBox('<svg><path d="M0 0h1v1z"/></svg>')).toThrow(
      /viewBox/,
    );
  });
});

describe('cropViewBox', () => {
  it('crops a landscape pad to its drawing plus 2%', () => {
    const { viewBox, aspect } = cropViewBox(WHITE, '0 128 64 64');
    const [x, y, w, h] = viewBox.split(' ').map(Number);
    expect(x).toBeCloseTo(10 - 0.88, 1);
    expect(y).toBeCloseTo(150 - 0.4, 1);
    expect(w! / h!).toBeCloseTo(aspect, 3);
    expect(aspect).toBeGreaterThan(2);
  });
  it('follows group transforms and skips clip paths', () => {
    const flipped =
      '<svg width="64" height="64"><defs><clipPath><path d="M-100-100h500v500z"/></clipPath></defs>' +
      '<g transform="matrix(1.25,0,0,-1.25,0,64)"><path d="M8 8h16v8H8z"/></g></svg>';
    const { viewBox } = cropViewBox(flipped, '0 0 64 64');
    const [x, y, w, h] = viewBox.split(' ').map(Number);
    expect(x).toBeCloseTo(10 - 0.4, 1);
    expect(y).toBeCloseTo(44 - 0.2, 1);
    expect(w).toBeCloseTo(20 * 1.04, 1);
    expect(h).toBeCloseTo(10 * 1.04, 1);
  });
  it('throws on an empty glyph', () => {
    expect(() =>
      cropViewBox('<svg viewBox="0 0 10 10"></svg>', '0 0 10 10'),
    ).toThrow(/empty/);
  });
});

describe('assertClean', () => {
  it.each(['#fff', 'style="x"', '<text', '<image', 'fill="#000"'])(
    'rejects %s',
    (bad) => {
      expect(() => assertClean('gb', `<path ${bad} d="M0 0"/>`)).toThrow(/gb/);
    },
  );
  it('accepts currentColor and none', () => {
    expect(() =>
      assertClean(
        'gb',
        '<path fill="currentColor" d="M0 0"/><path fill="none" d="M0 0"/>',
      ),
    ).not.toThrow();
  });
});

describe('SYSTEM_ICON_SOURCES', () => {
  it('maps 41 ids, each audited', () => {
    const entries = Object.entries(SYSTEM_ICON_SOURCES);
    expect(entries).toHaveLength(41);
    for (const [id, s] of entries) {
      expect(s.file, id).toMatch(/\.svg$/);
      expect(s.audit.trim(), id).not.toBe('');
    }
  });
});
