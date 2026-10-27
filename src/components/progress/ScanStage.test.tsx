import { render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { ScanStage } from './ScanStage';

const LABEL = 'Progreso del escaneo';

const solid = (container: HTMLElement) =>
  container.querySelectorAll('.ws-sleeve.is-solid').length;

describe('ScanStage', () => {
  const original = window.matchMedia;
  afterEach(() => {
    window.matchMedia = original;
  });

  it.each([
    [0, 0],
    [26, 2],
    [10_000, 12],
  ])('draws one solid slot per 25 ROMs: %i ROMs, %i slots', (roms, slots) => {
    const { container } = render(
      <ScanStage done={3} total={18} roms={roms} label={LABEL} />,
    );
    expect(solid(container)).toBe(slots);
  });

  it('is one image named by the given label', () => {
    render(<ScanStage done={0} total={0} roms={0} label={LABEL} />);
    expect(screen.getByRole('img', { name: LABEL })).toBeInTheDocument();
  });

  const offset = (container: HTMLElement) => {
    const slide = container.querySelector<SVGGElement>('.ws-slide');
    expect(slide).not.toBeNull();
    const m = /^translateX\((-?[\d.]+)px\)$/.exec(slide!.style.transform);
    expect(m).not.toBeNull();
    return Number(m![1]);
  };

  it('slides the folders under the reading line by the folders done', () => {
    const { container } = render(
      <ScanStage done={7} total={18} roms={0} label={LABEL} />,
    );
    expect(offset(container)).toBe(-24 * 7);
    expect(container.querySelector('.ws-route')).not.toBeNull();
  });

  it('only ever slides forward, also past every fifth folder', () => {
    const { container, rerender } = render(
      <ScanStage done={0} total={18} roms={0} label={LABEL} />,
    );
    let last = offset(container);
    for (let done = 1; done <= 12; done++) {
      rerender(<ScanStage done={done} total={18} roms={0} label={LABEL} />);
      const now = offset(container);
      expect(now).toBeLessThan(last);
      last = now;
    }
  });

  it('keeps the folder glyphs it already drew while it slides', () => {
    const { container, rerender } = render(
      <ScanStage done={4} total={18} roms={0} label={LABEL} />,
    );
    const before = container.querySelectorAll('.ws-slide > g')[5];
    rerender(<ScanStage done={5} total={18} roms={0} label={LABEL} />);
    expect([...container.querySelectorAll('.ws-slide > g')]).toContain(before);
  });

  it('draws no folder beyond the total', () => {
    const { container } = render(
      <ScanStage done={2} total={3} roms={0} label={LABEL} />,
    );
    expect(container.querySelectorAll('.ws-slide > g')).toHaveLength(3);
  });

  it('renders the same DOM under reduced motion (motion lives in CSS)', () => {
    const at = (reduce: boolean) => {
      window.matchMedia = vi.fn((query: string) => ({
        matches: reduce && query === '(prefers-reduced-motion: reduce)',
        media: query,
        onchange: null,
        addEventListener: vi.fn(),
        removeEventListener: vi.fn(),
        addListener: vi.fn(),
        removeListener: vi.fn(),
        dispatchEvent: vi.fn(),
      })) as unknown as typeof window.matchMedia;
      const { container, unmount } = render(
        <ScanStage done={4} total={18} roms={120} label={LABEL} />,
      );
      const html = container.innerHTML;
      unmount();
      return html;
    };
    expect(at(true)).toBe(at(false));
  });
});
