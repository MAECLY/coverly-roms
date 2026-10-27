import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { WorkStage } from './WorkStage';

describe('WorkStage', () => {
  it('draws a hidden svg frame with the shelf inside a labelled image', () => {
    const { container } = render(
      <WorkStage label="Extrayendo Next Tetris">
        <rect data-testid="art" />
      </WorkStage>,
    );
    const img = screen.getByRole('img', { name: 'Extrayendo Next Tetris' });
    const svg = container.querySelector('svg')!;
    expect(img.contains(svg)).toBe(true);
    expect(svg.getAttribute('aria-hidden')).toBe('true');
    expect(svg.getAttribute('focusable')).toBe('false');
    expect(svg.querySelector('.ws-shelf')).not.toBeNull();
    expect(svg.querySelector('.ws-cap')).not.toBeNull();
    expect(svg.contains(screen.getByTestId('art'))).toBe(true);
  });

  it('lays the overlay over the svg as HTML, never inside it', () => {
    const { container } = render(
      <WorkStage
        label="Stage"
        overlay={<span data-testid="overlay">icon</span>}
      >
        <g />
      </WorkStage>,
    );
    const svg = container.querySelector('svg')!;
    const overlay = screen.getByTestId('overlay');
    expect(svg.contains(overlay)).toBe(false);
    expect(screen.getByRole('img').contains(overlay)).toBe(true);
    expect(container.querySelector('foreignObject')).toBeNull();
  });
});
