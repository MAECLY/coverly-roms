import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { SystemIcon } from './SystemIcon';

const box = (el: Element) => [
  Number(
    el.getAttribute('width') ?? parseFloat((el as HTMLElement).style.width),
  ),
  Number(
    el.getAttribute('height') ?? parseFloat((el as HTMLElement).style.height),
  ),
];

describe('SystemIcon', () => {
  it('renders a decorative silhouette for a known id', () => {
    const { container } = render(<SystemIcon systemId="gb" size={16} />);
    const svg = container.querySelector('svg[data-system-icon="gb"]')!;
    expect(svg).toHaveAttribute('aria-hidden', 'true');
    expect(svg).toHaveAttribute('fill', 'currentColor');
    expect(svg).toHaveAttribute('preserveAspectRatio', 'xMidYMid meet');
    expect(box(svg)).toEqual([24, 16]);
  });
  it('is an image with a name when titled', () => {
    render(<SystemIcon systemId="gb" size={20} title="Game Boy" />);
    expect(screen.getByRole('img', { name: 'Game Boy' })).toHaveAttribute(
      'data-system-icon',
      'gb',
    );
  });
  it('draws an abbreviation without outline below 20px', () => {
    const { container } = render(<SystemIcon systemId="cps2" size={16} />);
    const m = container.querySelector('[data-system-icon="cps2"]')!;
    expect(m).toHaveTextContent('CPS2');
    expect(m).toHaveAttribute('aria-hidden', 'true');
    expect(m).not.toHaveClass('system-mount--outline');
    expect(box(m)).toEqual([24, 16]);
  });
  it('outlines from 20px and adds photo-corners from 24px', () => {
    const { container, rerender } = render(
      <SystemIcon systemId="cps2" size={20} />,
    );
    expect(container.querySelector('.system-mount--outline')).not.toBeNull();
    expect(container.querySelectorAll('.system-mount__corner')).toHaveLength(0);
    rerender(<SystemIcon systemId="cps2" size={24} />);
    expect(container.querySelectorAll('.system-mount__corner')).toHaveLength(4);
  });
  it('treats a CSS length as a small mount', () => {
    const { container } = render(<SystemIcon systemId="cps2" />);
    const m = container.querySelector<HTMLElement>(
      '[data-system-icon="cps2"]',
    )!;
    expect(m).not.toHaveClass('system-mount--outline');
    expect(m.style.height).toBe('1em');
    expect(m.style.width).toBe('1.5em');
  });
  it('sizes a silhouette from a CSS length through style', () => {
    const { container } = render(
      <SystemIcon systemId="gb" size="var(--row-icon)" />,
    );
    const svg = container.querySelector<SVGSVGElement>(
      'svg[data-system-icon="gb"]',
    )!;
    expect(svg).not.toHaveAttribute('width');
    expect(svg.style.height).toBe('var(--row-icon)');
    expect(svg.style.width).toBe('calc(var(--row-icon) * 1.5)');
  });
  it('reads a bare numeric string as pixels', () => {
    const { container } = render(<SystemIcon systemId="cps2" size="24" />);
    const m = container.querySelector<HTMLElement>(
      '[data-system-icon="cps2"]',
    )!;
    expect(box(m)).toEqual([36, 24]);
    expect(m).toHaveClass('system-mount--outline');
    expect(container.querySelectorAll('.system-mount__corner')).toHaveLength(4);
  });
  it('uses the first four letters of a user-defined id', () => {
    const { container } = render(<SystemIcon systemId="x68000" size={16} />);
    expect(
      container.querySelector('[data-system-icon="x68000"]'),
    ).toHaveTextContent('X680');
  });
  it('names a titled mount as an image', () => {
    render(<SystemIcon systemId="mame" size={20} title="Arcade (MAME)" />);
    expect(
      screen.getByRole('img', { name: 'Arcade (MAME)' }),
    ).toHaveTextContent('MAME');
  });
  it('does not read inherited keys as icons', () => {
    const { container } = render(
      <SystemIcon systemId="constructor" size={16} />,
    );
    expect(
      container.querySelector('[data-system-icon="constructor"]'),
    ).toHaveTextContent('CONS');
  });
  it('draws a folder for an unrecognised folder', () => {
    const { container } = render(<SystemIcon systemId={null} size={16} />);
    const f = container.querySelector('[data-system-icon="unknown"]')!;
    expect(f).not.toBeNull();
    expect(f).toHaveAttribute('aria-hidden', 'true');
    expect(box(f)).toEqual([24, 16]);
  });
  it('names a lone folder with the default label when the title is empty', () => {
    render(<SystemIcon systemId={null} size={20} title="" />);
    expect(
      screen.getByRole('img', { name: 'Folder with no known system' }),
    ).toHaveAttribute('data-system-icon', 'unknown');
  });
});
