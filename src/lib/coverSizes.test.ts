import { describe, expect, it } from 'vitest';

import { COVER_SIZES, columnsFor, stepSize } from './coverSizes';
import { COVER_SIZE_IDS } from './viewPrefs';

describe('COVER_SIZES', () => {
  it('keeps M at the tile height the grid has today', () => {
    expect(COVER_SIZES.m.tileW).toBe(168);
    expect(COVER_SIZES.m.tileH).toBe(296);
  });

  it('follows the stops of the spec (art 3:4 of the tile width)', () => {
    expect(COVER_SIZE_IDS.map((s) => COVER_SIZES[s].tileW)).toEqual([
      96, 128, 168, 220, 300,
    ]);
    expect(COVER_SIZE_IDS.map((s) => COVER_SIZES[s].artH)).toEqual([
      128, 171, 224, 293, 400,
    ]);
  });

  it('adds up the fixed parts of each tile', () => {
    for (const s of COVER_SIZE_IDS) {
      const m = COVER_SIZES[s];
      const text = m.nameLines * 16 + (m.meta === 'badge' ? 0 : 18);
      const extra = m.meta === 'line+size' ? 16 : 0;
      expect(m.tileH).toBe(16 + m.artH + (text > 0 ? 6 : 0) + text + extra);
      expect(m.artW).toBe(m.tileW - 16);
    }
  });

  it('shows the name and meta line only where there is room', () => {
    expect(COVER_SIZES.xs).toMatchObject({ nameLines: 0, meta: 'badge' });
    expect(COVER_SIZES.s).toMatchObject({ nameLines: 1, meta: 'badge' });
    expect(COVER_SIZES.m).toMatchObject({ nameLines: 2, meta: 'line' });
    expect(COVER_SIZES.l).toMatchObject({ nameLines: 2, meta: 'line' });
    expect(COVER_SIZES.xl).toMatchObject({ nameLines: 2, meta: 'line+size' });
  });
});

describe('stepSize', () => {
  it('moves one stop and clamps at both ends', () => {
    expect(stepSize('m', 1)).toBe('l');
    expect(stepSize('m', -1)).toBe('s');
    expect(stepSize('xl', 1)).toBe('xl');
    expect(stepSize('xs', -1)).toBe('xs');
  });
});

describe('columnsFor', () => {
  it('fits whole tiles and their gaps into the width', () => {
    expect(columnsFor(1200, 'm')).toBe(6);
    expect(columnsFor(1200, 'xs')).toBe(11);
    expect(columnsFor(1200, 'xl')).toBe(3);
    expect(columnsFor(1280, 'm')).toBe(7);
  });

  it('never returns fewer than one column', () => {
    expect(columnsFor(0, 'xl')).toBe(1);
    expect(columnsFor(50, 'xs')).toBe(1);
  });

  it('takes a custom gap', () => {
    expect(columnsFor(1200, 'm', 0)).toBe(7);
  });
});
