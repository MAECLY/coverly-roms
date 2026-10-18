import { describe, expect, it } from 'vitest';

import type { Section } from './grouping';
import {
  createTypeAhead,
  findByPrefix,
  groupJump,
  nearestByLeft,
  nextIndex,
  paneMove,
} from './keyboardNav';

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

const SECTIONS: readonly Section[] = [
  { key: 'a', label: 'A', start: 0, count: 3 },
  { key: 'b', label: 'B', start: 3, count: 0 },
  { key: 'c', label: 'C', start: 3, count: 4 },
  { key: 'd', label: 'D', start: 7, count: 2 },
];

describe('groupJump', () => {
  it('goes to the first item of the next section, skipping empty ones', () => {
    expect(groupJump(1, 1, SECTIONS)).toBe(3);
    expect(groupJump(1, 4, SECTIONS)).toBe(7);
  });
  it('goes to the first item of the previous section', () => {
    expect(groupJump(-1, 8, SECTIONS)).toBe(3);
    expect(groupJump(-1, 3, SECTIONS)).toBe(0);
    expect(groupJump(-1, 5, SECTIONS)).toBe(0);
  });
  it('returns null when there is no section that way', () => {
    expect(groupJump(-1, 2, SECTIONS)).toBeNull();
    expect(groupJump(1, 8, SECTIONS)).toBeNull();
    expect(groupJump(1, 0, [])).toBeNull();
  });
  it('starts at the first section with no selection', () => {
    expect(groupJump(1, -1, SECTIONS)).toBe(0);
    expect(groupJump(-1, -1, SECTIONS)).toBeNull();
  });
});

describe('createTypeAhead', () => {
  it('keeps typing within 500 ms in one buffer', () => {
    const ta = createTypeAhead();
    expect(ta.push('s', 1000)).toBe('s');
    expect(ta.push('u', 1400)).toBe('su');
  });
  it('starts a new buffer after the silence', () => {
    const ta = createTypeAhead();
    ta.push('s', 1000);
    expect(ta.push('u', 1600)).toBe('u');
  });
  it('takes a custom silence', () => {
    const ta = createTypeAhead(100);
    ta.push('a', 0);
    expect(ta.push('b', 150)).toBe('b');
  });
});

describe('findByPrefix', () => {
  const names = ['Alpha', 'Équipe', 'beta', 'Echo', 'alpine'];
  const at = (i: number) => names[i] ?? '';
  it('searches after `from` and wraps', () => {
    expect(findByPrefix(at, names.length, 0, 'al')).toBe(4);
    expect(findByPrefix(at, names.length, 4, 'al')).toBe(0);
    expect(findByPrefix(at, names.length, -1, 'b')).toBe(2);
  });
  it('ignores case and diacritics', () => {
    expect(findByPrefix(at, names.length, 0, 'e')).toBe(1);
    expect(findByPrefix(at, names.length, 1, 'E')).toBe(3);
    expect(findByPrefix(at, names.length, 0, 'éq')).toBe(1);
  });
  it('keeps `from` only when nothing else matches', () => {
    expect(findByPrefix(at, names.length, 2, 'be')).toBe(2);
  });
  it('returns null without a match', () => {
    expect(findByPrefix(at, names.length, 0, 'z')).toBeNull();
    expect(findByPrefix(at, 0, -1, 'a')).toBeNull();
    expect(findByPrefix(at, names.length, 0, '')).toBeNull();
  });
});

describe('nearestByLeft', () => {
  it('picks the candidate nearest to the left edge', () => {
    expect(nearestByLeft(105, [0, 100, 220])).toBe(1);
    expect(nearestByLeft(500, [0, 100, 220])).toBe(2);
    expect(nearestByLeft(-20, [0, 100, 220])).toBe(0);
  });
  it('prefers the first of two equal distances', () => {
    expect(nearestByLeft(50, [0, 100])).toBe(0);
  });
  it('returns -1 with no candidates', () => {
    expect(nearestByLeft(10, [])).toBe(-1);
  });
});

describe('paneMove', () => {
  it('goes right only from a branch and not past the last pane', () => {
    expect(paneMove('ArrowRight', 0, 3, true)).toBe(1);
    expect(paneMove('ArrowRight', 0, 3, false)).toBeNull();
    expect(paneMove('ArrowRight', 2, 3, true)).toBeNull();
  });
  it('goes left until the first pane', () => {
    expect(paneMove('ArrowLeft', 2, 3, false)).toBe(1);
    expect(paneMove('ArrowLeft', 0, 3, true)).toBeNull();
  });
});
