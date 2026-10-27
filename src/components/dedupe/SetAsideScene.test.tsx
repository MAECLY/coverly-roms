import { act, render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { SetAsideScene } from './SetAsideScene';

type Props = Parameters<typeof SetAsideScene>[0];

const base: Props = {
  done: 0,
  total: 40,
  stopped: false,
  folder: '_Duplicados',
};

function setup(props: Partial<Props> = {}) {
  const view = render(<SetAsideScene {...base} {...props} />);
  const rerender = (next: Partial<Props>) =>
    view.rerender(<SetAsideScene {...base} {...props} {...next} />);
  const moving = () =>
    [...view.container.querySelectorAll('[data-moving]')].map(
      (el) => el.getAttribute('data-flight') ?? '',
    );
  const count = () =>
    view.container.querySelector('[data-scene-count]')?.textContent ?? '';
  return { ...view, rerender, moving, count };
}

/** Steps fake time in small slices, collecting every flight seen. */
function watch(ms: number, moving: () => string[]): Set<string> {
  const seen = new Set<string>();
  for (let t = 0; t < ms; t += 10) {
    act(() => {
      vi.advanceTimersByTime(10);
    });
    for (const id of moving()) seen.add(id);
  }
  return seen;
}

function reduceMotion(on: boolean) {
  vi.spyOn(window, 'matchMedia').mockImplementation(
    (query: string) =>
      ({
        matches: on && query.includes('reduce'),
        media: query,
        onchange: null,
        addListener: () => undefined,
        removeListener: () => undefined,
        addEventListener: () => undefined,
        removeEventListener: () => undefined,
        dispatchEvent: () => false,
      }) as MediaQueryList,
  );
}

describe('SetAsideScene', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it('is decoration: aria-hidden, the folder name and the count in mono', () => {
    const { container, count } = setup({ done: 3 });
    const svg = container.querySelector('svg')!;
    expect(svg).toHaveAttribute('aria-hidden', 'true');
    expect(svg.textContent).toContain('_Duplicados');
    // A resumed apply starts from where it is, without replaying.
    expect(count()).toBe('3');
    expect(container.querySelectorAll('[data-moving]')).toHaveLength(0);
    expect(container.querySelector('[role="status"]')).toBeNull();
  });

  it('lifts one slot per copy, in sequence', () => {
    const { rerender, moving, count } = setup();
    act(() => rerender({ done: 1 }));
    const first = watch(300, moving);
    expect(first.size).toBe(1);
    expect(count()).toBe('1');
    act(() => rerender({ done: 2 }));
    const second = watch(300, moving);
    expect(second.size).toBeGreaterThanOrEqual(1);
    const all = new Set([...first, ...second]);
    expect(all.size).toBe(2);
    expect(count()).toBe('2');
    // Nothing loops on its own once the queue is drained.
    act(() => {
      vi.advanceTimersByTime(1000);
    });
    expect(watch(2000, moving).size).toBe(0);
  });

  it('coalesces a burst to at most four lifts a second and shows the true count', () => {
    const { rerender, moving, count } = setup({ done: 2 });
    act(() => rerender({ done: 40 }));
    expect(count()).toBe('40');
    const seen = watch(990, moving);
    expect(seen.size).toBeGreaterThan(0);
    expect(seen.size).toBeLessThanOrEqual(4);
    expect(count()).toBe('40');
  });

  it('freezes when stopped', () => {
    const { rerender, moving, container } = setup();
    act(() => rerender({ done: 20 }));
    act(() => {
      vi.advanceTimersByTime(260);
    });
    act(() => rerender({ done: 20, stopped: true }));
    const frozen = moving();
    const later = watch(2000, moving);
    // No new flight starts; the ones in the air stay where they are.
    expect([...later].every((id) => frozen.includes(id))).toBe(true);
    expect(container.querySelector('svg')).toHaveAttribute(
      'data-stopped',
      'true',
    );
  });

  it('thaws after a refused stop: no frozen flight lingers, the count catches up', () => {
    const { rerender, moving, count, container } = setup();
    act(() => rerender({ done: 3 }));
    act(() => {
      vi.advanceTimersByTime(100);
    });
    act(() => rerender({ done: 3, stopped: true }));
    expect(moving().length).toBeGreaterThan(0);
    act(() => rerender({ done: 4, stopped: false }));
    expect(count()).toBe('4');
    expect(container.querySelector('svg')).not.toHaveAttribute('data-stopped');
    act(() => {
      vi.advanceTimersByTime(3000);
    });
    expect(moving()).toHaveLength(0);
    expect(container.querySelector('.dup-scene__slot--missing')).toBeNull();
  });

  it('does not move under reduced motion; the count still updates', () => {
    reduceMotion(true);
    const { rerender, moving, count } = setup();
    act(() => rerender({ done: 1 }));
    act(() => rerender({ done: 7 }));
    expect(watch(1500, moving).size).toBe(0);
    expect(count()).toBe('7');
  });

  it('closes the folder tab when everything is in', () => {
    const { rerender, container } = setup({ done: 39 });
    expect(container.querySelector('svg')).not.toHaveAttribute(
      'data-closed',
      'true',
    );
    act(() => rerender({ done: 40 }));
    act(() => {
      vi.advanceTimersByTime(1500);
    });
    expect(container.querySelector('svg')).toHaveAttribute(
      'data-closed',
      'true',
    );
  });
});
