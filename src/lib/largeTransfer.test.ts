import { describe, expect, it } from 'vitest';

import type {
  LargeTransferDetail,
  OrganizeItem,
  OrganizePlan,
} from '@/ipc/types';

import {
  acceptedBytes,
  asksRest,
  restDetail,
  roomAfter,
  roomForItem,
  stillMoves,
} from './largeTransfer';

const DETAIL: LargeTransferDetail = {
  bytes: 187_420_000_000,
  items: 12,
  copies: true,
  free: 412_100_000_000,
  volume: 'TinyExt',
  huge: [
    {
      item: 1,
      name: 'CUSA00900',
      system: 'ps4',
      bytes: 62_920_000_000,
      extract: false,
      followers: [3],
    },
    {
      item: 5,
      name: 'Forza Horizon 5',
      system: 'xboxseries',
      bytes: 110_400_000_000,
      extract: false,
      followers: [],
    },
  ],
  rest: 14_100_000_000,
};

describe('large transfer numbers', () => {
  it('keeps the apply’s 1 % margin, rounded up like Rust', () => {
    expect(roomAfter(1_000, 100)).toEqual({ fits: true, left: 899 });
    expect(roomAfter(1_000, 101)).toEqual({ fits: true, left: 897 });
    expect(roomAfter(100, 100)).toEqual({ fits: false, missing: 1 });
  });

  it('counts the rest and the items accepted before the one asked', () => {
    expect(acceptedBytes(DETAIL, [5])).toBe(110_400_000_000);
    expect(roomForItem(DETAIL, [], DETAIL.huge[0]!)).toEqual({
      fits: true,
      left: 412_100_000_000 - 77_020_000_000 - 770_200_000,
    });
    expect(
      roomForItem({ ...DETAIL, free: null }, [], DETAIL.huge[0]!),
    ).toBeNull();
  });

  it('asks the rest only with huge items and 10 GB or more of it', () => {
    expect(asksRest(DETAIL)).toBe(true);
    expect(asksRest({ ...DETAIL, rest: 9_999_999_999 })).toBe(false);
    expect(asksRest({ ...DETAIL, huge: [] })).toBe(false);
    expect(restDetail(DETAIL)).toMatchObject({
      bytes: 14_100_000_000,
      items: 9,
      huge: [],
    });
    const none = { ...DETAIL, huge: [] };
    expect(restDetail(none)).toBe(none);
  });

  it('tells whether anything moves once items are declined', () => {
    const item = (id: number, action: OrganizeItem['action']) =>
      ({
        id,
        action,
        target: action === 'auto' ? `t/${id}` : null,
      }) as OrganizeItem;
    const plan = {
      items: [item(1, 'auto'), item(3, 'auto'), item(4, 'left')],
    } as OrganizePlan;
    // Declining the base takes its follower with it.
    expect(stillMoves(plan, DETAIL, [1])).toBe(false);
    expect(stillMoves(plan, DETAIL, [])).toBe(true);
    const aside = {
      items: [...plan.items, item(9, 'set-aside')],
    } as OrganizePlan;
    expect(stillMoves(aside, DETAIL, [1])).toBe(true);
  });
});
