import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { SystemLabel } from './SystemLabel';

describe('SystemLabel', () => {
  it('reads as the visible name only, with a decorative icon', () => {
    const { container } = render(
      <SystemLabel systemId="gb" name="Game Boy" size={16} />,
    );
    const name = screen.getByText('Game Boy');
    expect(name).toHaveClass('system-label__name');
    const label = container.querySelector('.system-label')!;
    expect(label).toHaveTextContent(/^Game Boy$/);
    const icon = label.querySelector('[data-system-icon="gb"]')!;
    expect(icon).toHaveAttribute('aria-hidden', 'true');
    expect(icon).not.toHaveAttribute('role');
    expect(label.firstElementChild).toBe(icon);
  });

  it('gives its container the visible name as the accessible name', () => {
    render(
      <>
        <button type="button">
          <SystemLabel systemId="gb" name="Game Boy" size={16} />
        </button>
        <button type="button">
          <SystemLabel systemId="cps2" name="CPS-2" size={24} trailing />
        </button>
        <button type="button">
          <SystemLabel systemId={null} name="Homebrew" size={16} />
        </button>
      </>,
    );
    expect(screen.getByRole('button', { name: 'Game Boy' })).toBeVisible();
    expect(screen.getByRole('button', { name: 'CPS-2' })).toBeVisible();
    expect(screen.getByRole('button', { name: 'Homebrew' })).toBeVisible();
    expect(screen.queryByRole('img')).toBeNull();
  });

  it('puts a decorative icon after the name when trailing, with a tooltip', () => {
    const { container } = render(
      <SystemLabel systemId="gb" name="Game Boy" size={20} trailing />,
    );
    const label = container.querySelector('.system-label')!;
    const icon = label.querySelector('[data-system-icon]')!;
    expect(label.lastElementChild).toBe(icon);
    expect(icon).toHaveAttribute('aria-hidden', 'true');
    expect(icon).not.toHaveAttribute('aria-label');
    expect(label).toHaveAttribute('title', 'Game Boy');
  });

  it('has no tooltip when leading', () => {
    const { container } = render(
      <SystemLabel systemId="gb" name="Game Boy" size={16} />,
    );
    expect(container.querySelector('.system-label')).not.toHaveAttribute(
      'title',
    );
  });

  it('passes the size to the icon and keeps a class', () => {
    const { container } = render(
      <SystemLabel systemId="gb" name="Game Boy" size={20} className="x" />,
    );
    expect(container.querySelector('.system-label')).toHaveClass('x');
    expect(container.querySelector('svg')).toHaveAttribute('height', '20');
  });

  it('draws a folder for an unrecognised system', () => {
    const { container } = render(
      <SystemLabel systemId={null} name="Homebrew" size={16} />,
    );
    expect(
      container.querySelector('[data-system-icon="unknown"]'),
    ).toHaveAttribute('aria-hidden', 'true');
    expect(screen.getByText('Homebrew')).toBeInTheDocument();
  });
});
