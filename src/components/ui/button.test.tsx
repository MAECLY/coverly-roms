import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { Button, buttonVariants } from './button';

describe('Button', () => {
  it('is a native type="button" that activates with Enter and Space', async () => {
    const onClick = vi.fn();
    render(<Button onClick={onClick}>Apply 41 covers</Button>);
    const button = screen.getByRole('button', { name: 'Apply 41 covers' });
    expect(button).toHaveAttribute('type', 'button');
    button.focus();
    await userEvent.keyboard('{Enter}');
    await userEvent.keyboard(' ');
    expect(onClick).toHaveBeenCalledTimes(2);
  });

  it('keeps an explicit submit type', () => {
    render(<Button type="submit">Save</Button>);
    expect(screen.getByRole('button')).toHaveAttribute('type', 'submit');
  });

  it('renders its child with asChild and no button type', () => {
    render(
      <Button asChild variant="link">
        <a href="https://example.org">Docs</a>
      </Button>,
    );
    const link = screen.getByRole('link', { name: 'Docs' });
    expect(link).not.toHaveAttribute('type');
    expect(link.className).toContain('underline');
  });

  it('loading disables, sets aria-busy and keeps the name', async () => {
    const onClick = vi.fn();
    render(
      <Button loading onClick={onClick}>
        Save
      </Button>,
    );
    const button = screen.getByRole('button', { name: 'Save' });
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute('aria-busy', 'true');
    await userEvent.click(button);
    expect(onClick).not.toHaveBeenCalled();
  });

  it('maps variants and sizes to the brand control heights', () => {
    expect(buttonVariants({ variant: 'primary', size: 'sm' })).toContain(
      'h-control-sm',
    );
    expect(buttonVariants({ size: 'lg' })).toContain('h-control-lg');
    expect(buttonVariants({ variant: 'destructive' })).toContain(
      'bg-destructive',
    );
    expect(buttonVariants({})).toContain('focus-visible:outline-ring');
  });

  it('filled buttons keep an edge in forced colours', () => {
    // Forced colours repaint fills with Canvas: without a border a filled
    // button reads as plain text.
    for (const variant of ['primary', 'secondary', 'destructive'] as const)
      expect(buttonVariants({ variant })).toContain(
        'forced-colors:border-[ButtonText]',
      );
  });
});
