import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import markSvg from '../../../design/icon/mark.svg?raw';
import { Logo, LOGO_NAME } from './Logo';

describe('Logo', () => {
  it('shows the mark and the wordmark, hiding the mark from assistive tech', () => {
    render(<Logo />);
    expect(screen.getByText(LOGO_NAME)).toBeInTheDocument();
    const mark = screen.getByTestId('logo-mark');
    expect(mark).toHaveAttribute('aria-hidden', 'true');
    expect(mark.style.height).toBe('1.4em');
  });

  it('labels the mark as an image when the wordmark is hidden', () => {
    render(<Logo showWordmark={false} size={32} />);
    expect(screen.queryByText(LOGO_NAME)).not.toBeInTheDocument();
    const mark = screen.getByRole('img', { name: LOGO_NAME });
    expect(mark.style.height).toBe('32px');
  });

  it('draws in currentColor tied to the brand primary', () => {
    render(<Logo />);
    const mark = screen.getByTestId('logo-mark');
    expect(mark).toHaveAttribute('fill', 'currentColor');
    expect(mark.getAttribute('style')).toContain('var(--cr-accent)');
  });

  it('keeps the same geometry as design/icon/mark.svg', () => {
    const { container } = render(<Logo />);
    const rects = container.querySelectorAll('rect');
    expect(rects).toHaveLength(3);
    for (const rect of rects) {
      const x = rect.getAttribute('x');
      const y = rect.getAttribute('y');
      const w = rect.getAttribute('width');
      expect(markSvg).toContain(`x="${x}" y="${y}" width="${w}"`);
    }
  });
});
