import { describe, expect, it } from 'vitest';

import type { EntryView } from '@/ipc/types';

import { sortEntries } from './sort';

const e = (
  id: number,
  name: string,
  system: number,
  size: number,
  mtime: number,
) => ({ id, name, system, size, mtime }) as EntryView;
const list = [e(0, 'b10', 1, 30, 1), e(1, 'B2', 0, 10, 3), e(2, 'a', 1, 20, 2)];
const names = ['Super Nintendo', 'Game Boy'];
const ids = (xs: EntryView[]) => xs.map((x) => x.id);

describe('sortEntries', () => {
  it('keeps library order by default', () => {
    expect(
      ids(sortEntries(list, { key: 'library', dir: 'asc' }, names)),
    ).toEqual([0, 1, 2]);
  });
  it('sorts names naturally and case-insensitively', () => {
    expect(ids(sortEntries(list, { key: 'name', dir: 'asc' }, names))).toEqual([
      2, 1, 0,
    ]);
  });
  it('sorts by system name, size and date, both directions', () => {
    expect(
      ids(sortEntries(list, { key: 'system', dir: 'asc' }, names)),
    ).toEqual([2, 0, 1]);
    expect(ids(sortEntries(list, { key: 'size', dir: 'desc' }, names))).toEqual(
      [0, 2, 1],
    );
    expect(
      ids(sortEntries(list, { key: 'mtime', dir: 'desc' }, names)),
    ).toEqual([1, 2, 0]);
  });
  it('never mutates its input', () => {
    sortEntries(list, { key: 'name', dir: 'asc' }, names);
    expect(ids(list)).toEqual([0, 1, 2]);
  });
});
