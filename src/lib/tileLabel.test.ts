import { describe, expect, it } from 'vitest';

import { titleWithoutRegions } from './tileLabel';

describe('titleWithoutRegions', () => {
  it('drops the region group and keeps the other tags', () => {
    expect(titleWithoutRegions('Golden Sun (USA, Europe)')).toBe('Golden Sun');
    expect(titleWithoutRegions('Tetris (World) (Rev 1)')).toBe(
      'Tetris (Rev 1)',
    );
    expect(titleWithoutRegions('Super Metroid (Japan, USA) (En,Ja)')).toBe(
      'Super Metroid (En,Ja)',
    );
  });

  it('keeps a first group that is not a region list', () => {
    expect(titleWithoutRegions('Doom (Beta) (USA)')).toBe('Doom (Beta) (USA)');
    expect(titleWithoutRegions('Castle Heroes')).toBe('Castle Heroes');
  });

  it('never returns an empty title', () => {
    expect(titleWithoutRegions('(Europe)')).toBe('(Europe)');
  });
});
