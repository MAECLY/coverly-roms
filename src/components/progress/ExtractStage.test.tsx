import { act, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';

import { ExtractStage, type ExtractStageProps } from './ExtractStage';
import {
  FLY_CAP,
  initialStage,
  stageReducer,
  type StageState,
} from './stageQueue';

const event = (item: number, done: number, instant = false) =>
  ({ type: 'event', item, done, instant }) as const;

describe('stageReducer', () => {
  it('takes the first event of an item as the baseline, with no replay', () => {
    expect(stageReducer(initialStage, event(5, 4))).toEqual({
      item: 5,
      seen: 4,
      landed: 0,
      flying: [],
    });
  });

  it('flies at most the cap and lands the rest at once', () => {
    const a = stageReducer(initialStage, event(5, 4));
    const b = stageReducer(a, event(5, 14));
    expect(FLY_CAP).toBe(3);
    expect(b.flying).toHaveLength(3);
    expect(b.landed).toBe(7);
    expect(b.seen).toBe(14);
    // The newest entries are the ones that fly, staggered from 0.
    expect(b.flying.map((f) => f.id)).toEqual([11, 12, 13]);
    expect(b.flying.map((f) => f.g)).toEqual([0, 1, 2]);
  });

  it('lands one flight, then drains the rest', () => {
    const b = stageReducer(
      stageReducer(initialStage, event(5, 4)),
      event(5, 14),
    );
    const c = stageReducer(b, { type: 'landed', id: b.flying[0]!.id });
    expect(c.landed).toBe(8);
    expect(c.flying).toHaveLength(2);
    // A stale landing (already landed) changes nothing.
    expect(stageReducer(c, { type: 'landed', id: b.flying[0]!.id })).toBe(c);
    const d = stageReducer(c, { type: 'drain' });
    expect(d.landed).toBe(10);
    expect(d.flying).toEqual([]);
  });

  it('flies nothing when instant', () => {
    const a = stageReducer(initialStage, event(5, 4));
    const b = stageReducer(a, event(5, 14, true));
    expect(b.flying).toEqual([]);
    expect(b.landed).toBe(10);
  });

  it('ignores an event that does not move forward', () => {
    const a = stageReducer(initialStage, event(5, 4));
    expect(stageReducer(a, event(5, 4))).toBe(a);
    expect(stageReducer(a, event(5, 2))).toBe(a);
  });

  it('honours a smaller cap (moves fly one at a time)', () => {
    const a = stageReducer(initialStage, event(1, 0));
    const b = stageReducer(a, { ...event(1, 3), cap: 1 });
    expect(b.flying).toHaveLength(1);
    expect(b.landed).toBe(2);
  });

  it('resets on a new item', () => {
    const b: StageState = stageReducer(
      stageReducer(initialStage, event(5, 4)),
      event(5, 14),
    );
    expect(stageReducer(b, event(6, 0))).toEqual({
      item: 6,
      seen: 0,
      landed: 0,
      flying: [],
    });
  });
});

const original = window.matchMedia;

function stubReducedMotion(reduce: boolean) {
  window.matchMedia = vi.fn(
    (query: string) =>
      ({
        matches: reduce && query === '(prefers-reduced-motion: reduce)',
        media: query,
        onchange: null,
        addListener: () => undefined,
        removeListener: () => undefined,
        addEventListener: () => undefined,
        removeEventListener: () => undefined,
        dispatchEvent: () => false,
      }) as MediaQueryList,
  ) as unknown as typeof window.matchMedia;
}

const BASE: ExtractStageProps = {
  label: 'Extracción de Tetris Plus.zip hacia la carpeta de PlayStation',
  itemId: 5,
  systemId: 'psx',
  entries: { done: 4, total: 12 },
  phase: 'decoding',
  cancelling: false,
};

function renderStage(props: Partial<ExtractStageProps> = {}) {
  const ui = (p: Partial<ExtractStageProps>) => (
    <I18nProvider locale="es">
      <ExtractStage {...BASE} {...p} />
    </I18nProvider>
  );
  const r = render(ui(props));
  return {
    ...r,
    update: (next: Partial<ExtractStageProps>) =>
      r.rerender(ui({ ...props, ...next })),
  };
}

afterEach(() => {
  window.matchMedia = original;
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe('ExtractStage', () => {
  it('names the image and lays the system icon and count on the plate', () => {
    const { container } = renderStage();
    const img = screen.getByRole('img', { name: BASE.label });
    const svg = container.querySelector('svg')!;
    const icon = container.querySelector('[data-system-icon="psx"]')!;
    expect(icon).not.toBeNull();
    expect(img.contains(icon)).toBe(true);
    expect(svg.contains(icon)).toBe(false);
    expect(img).toHaveTextContent('4/12');
  });

  it('flies up to three sleeves when motion is allowed', () => {
    stubReducedMotion(false);
    const { container, update } = renderStage();
    update({ entries: { done: 14, total: 20 } });
    expect(container.querySelectorAll('.ws-fly-x')).toHaveLength(3);
    expect(container.querySelector('.ws-lid.is-open')).not.toBeNull();
    expect(container.querySelectorAll('.ws-march')).toHaveLength(1);
  });

  it('lands a sleeve whose animation never ends after the safety timer', () => {
    vi.useFakeTimers();
    stubReducedMotion(false);
    const { container, update } = renderStage();
    update({ entries: { done: 6, total: 20 } });
    expect(container.querySelectorAll('.ws-fly-x')).toHaveLength(2);
    act(() => {
      vi.advanceTimersByTime(1500);
    });
    expect(container.querySelectorAll('.ws-fly-x')).toHaveLength(1);
    act(() => {
      vi.advanceTimersByTime(1500);
    });
    expect(container.querySelectorAll('.ws-fly-x')).toHaveLength(0);
  });

  it('keeps the safety timer of the oldest sleeve while events keep coming', () => {
    vi.useFakeTimers();
    stubReducedMotion(false);
    const { container, update } = renderStage();
    update({ entries: { done: 5, total: 40 } });
    expect(container.querySelectorAll('.ws-fly-x')).toHaveLength(1);
    // Two more sleeves take off before the first one's timer is due.
    for (const done of [6, 7]) {
      act(() => {
        vi.advanceTimersByTime(500);
      });
      update({ entries: { done, total: 40 } });
    }
    act(() => {
      vi.advanceTimersByTime(501);
    });
    // 1501 ms after it left, the first sleeve has landed all the same.
    expect(container.querySelectorAll('.ws-fly-x')).toHaveLength(2);
    expect(container.querySelectorAll('.ws-spine')).toHaveLength(1);
  });

  it('flies nothing under reduced motion', () => {
    stubReducedMotion(true);
    const { container, update } = renderStage();
    update({ entries: { done: 14, total: 20 } });
    expect(container.querySelector('.ws-fly-x')).toBeNull();
    // The landed spines still show what happened.
    expect(container.querySelectorAll('.ws-spine').length).toBeGreaterThan(0);
  });

  it('stops the march and every flight while cancelling', () => {
    stubReducedMotion(false);
    const { container, update } = renderStage();
    update({ entries: { done: 8, total: 20 } });
    expect(container.querySelector('.ws-fly-x')).not.toBeNull();
    update({ entries: { done: 8, total: 20 }, cancelling: true });
    expect(container.querySelector('.ws-march')).toBeNull();
    expect(container.querySelector('.ws-fly-x')).toBeNull();
    expect(container.querySelector('.ws-lid.is-open')).toBeNull();
  });

  it('closes the lid and draws the check while checking', () => {
    const { container } = renderStage({
      entries: { done: 12, total: 12 },
      phase: 'checking',
    });
    const lid = container.querySelector('.ws-lid')!;
    expect(lid).not.toBeNull();
    expect(lid).not.toHaveClass('is-open');
    expect(container.querySelector('path.ws-check')).not.toBeNull();
    expect(container.querySelector('.ws-march')).toBeNull();
  });

  it('draws a stack of solid sleeves and no system icon for moves', () => {
    stubReducedMotion(false);
    const { container, update } = renderStage({
      variant: 'move',
      systemId: null,
      entries: { done: 0, total: 40 },
    });
    expect(container.querySelector('.ws-box')).toBeNull();
    expect(container.querySelector('[data-system-icon]')).toBeNull();
    update({ entries: { done: 3, total: 40 } });
    expect(container.querySelectorAll('.ws-fly-x')).toHaveLength(1);
  });

  it('shows no count without per-entry data', () => {
    const { container } = renderStage({ entries: null });
    expect(screen.getByRole('img')).not.toHaveTextContent('/');
    expect(container.querySelector('.ws-march')).toBeNull();
  });
});
