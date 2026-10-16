import { describe, expect, it } from 'vitest';

import { nextIndex } from './keyboardNav';

describe('nextIndex', () => {
  it('moves in a grid and clamps at the edges', () => {
    expect(nextIndex('ArrowRight', 0, 10, 4, 2)).toBe(1);
    expect(nextIndex('ArrowLeft', 0, 10, 4, 2)).toBe(0);
    expect(nextIndex('ArrowDown', 1, 10, 4, 2)).toBe(5);
    expect(nextIndex('ArrowDown', 8, 10, 4, 2)).toBe(9);
    expect(nextIndex('ArrowUp', 5, 10, 4, 2)).toBe(1);
    expect(nextIndex('ArrowUp', 2, 10, 4, 2)).toBe(0);
    expect(nextIndex('PageDown', 0, 100, 4, 3)).toBe(12);
    expect(nextIndex('PageUp', 5, 100, 4, 3)).toBe(0);
    expect(nextIndex('Home', 7, 10, 4, 2)).toBe(0);
    expect(nextIndex('End', 0, 10, 4, 2)).toBe(9);
  });
  it('starts at the first item with no selection', () => {
    expect(nextIndex('ArrowDown', -1, 10, 4, 2)).toBe(0);
    expect(nextIndex('End', -1, 10, 4, 2)).toBe(9);
  });
  it('ignores other keys and empty lists', () => {
    expect(nextIndex('a', 0, 10, 4, 2)).toBeNull();
    expect(nextIndex('ArrowDown', -1, 0, 4, 2)).toBeNull();
  });
});
