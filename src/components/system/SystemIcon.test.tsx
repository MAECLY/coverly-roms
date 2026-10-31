import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { Button } from '@/components/ui/button';

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
  it('renders a batch 1 silhouette for ps2', () => {
    const { container } = render(<SystemIcon systemId="ps2" size={16} />);
    expect(
      container.querySelector('svg[data-system-icon="ps2"]'),
    ).not.toBeNull();
  });
  it('mounts the batch 1 systems with no silhouette under their label', () => {
    const { container, rerender } = render(
      <SystemIcon systemId="wasm4" size={16} />,
    );
    const w4 = container.querySelector('[data-system-icon="wasm4"]')!;
    expect(w4.tagName.toLowerCase()).not.toBe('svg');
    expect(w4).toHaveTextContent('W4');
    rerender(<SystemIcon systemId="model2" size={16} />);
    expect(
      container.querySelector('[data-system-icon="model2"]'),
    ).toHaveTextContent('MDL2');
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
    const { container } = render(<SystemIcon systemId="pc6001" size={16} />);
    expect(
      container.querySelector('[data-system-icon="pc6001"]'),
    ).toHaveTextContent('PC60');
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
  it('keeps its 1.5x box in style, so a Button cannot square it', () => {
    // Button sizes every svg it holds to 1rem through a class rule, which
    // beats width/height attributes; only an inline style holds the box.
    render(
      <Button>
        <SystemIcon systemId="gb" size={16} />
        <SystemIcon systemId={null} size={16} />
      </Button>,
    );
    const svg = document.querySelector<SVGElement>(
      'svg[data-system-icon="gb"]',
    )!;
    expect(svg.style.width).toBe('24px');
    expect(svg.style.height).toBe('16px');
    const folder = document.querySelector<HTMLElement>(
      '[data-system-icon="unknown"] svg',
    )!;
    expect(folder.style.width).toBe('16px');
  });
});
