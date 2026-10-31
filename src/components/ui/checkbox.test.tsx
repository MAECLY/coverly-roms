import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { Checkbox } from './checkbox';
import { Label } from './label';
import { RadioGroup, RadioGroupItem } from './radio-group';
import { Switch } from './switch';

describe('Checkbox', () => {
  it('toggles with Space and reports aria-checked, incl. indeterminate', async () => {
    const onChange = vi.fn();
    const { rerender } = render(
      <>
        <Checkbox id="c" onCheckedChange={onChange} />
        <Label htmlFor="c">GBA</Label>
      </>,
    );
    const box = screen.getByRole('checkbox', { name: 'GBA' });
    box.focus();
    await userEvent.keyboard(' ');
    expect(box).toBeChecked();
    expect(onChange).toHaveBeenLastCalledWith(true);
    rerender(
      <>
        <Checkbox id="c" checked="indeterminate" />
        <Label htmlFor="c">GBA</Label>
      </>,
    );
    expect(screen.getByRole('checkbox')).toHaveAttribute(
      'aria-checked',
      'mixed',
    );
  });

  it('draws the off state as a dashed SVG slot, not a CSS dashed border', () => {
    render(<Checkbox aria-label="x" />);
    const rect = screen.getByRole('checkbox').querySelector('rect');
    expect(rect).toHaveAttribute('stroke-dasharray', '3 2');
  });
});

describe('RadioGroup', () => {
  it('moves and selects with arrow keys', async () => {
    const onValueChange = vi.fn();
    render(
      <RadioGroup
        aria-label="Overwrite"
        defaultValue="never"
        onValueChange={onValueChange}
      >
        <RadioGroupItem value="never" aria-label="Never" />
        <RadioGroupItem value="always" aria-label="Always" />
      </RadioGroup>,
    );
    expect(
      screen.getByRole('radiogroup', { name: 'Overwrite' }),
    ).toBeInTheDocument();
    screen.getByRole('radio', { name: 'Never' }).focus();
    await userEvent.keyboard('{ArrowDown}');
    expect(screen.getByRole('radio', { name: 'Always' })).toHaveFocus();
    await userEvent.keyboard(' ');
    expect(screen.getByRole('radio', { name: 'Always' })).toBeChecked();
    expect(onValueChange).toHaveBeenLastCalledWith('always');
  });
});

describe('RadioGroup in forced colours', () => {
  it('draws the chosen dot in a system colour forced colours keep', () => {
    render(
      <RadioGroup defaultValue="a" aria-label="Copia">
        <RadioGroupItem value="a" aria-label="A" />
      </RadioGroup>,
    );
    // A background-only dot vanishes when forced colours paint backgrounds
    // with Canvas: it opts out and paints itself CanvasText.
    const dot = screen.getByRole('radio', { name: 'A' }).firstElementChild!;
    expect(dot.className).toContain('forced-colors:forced-color-adjust-none');
    expect(dot.className).toContain('forced-colors:bg-[CanvasText]');
  });
});

describe('Switch', () => {
  it('is role="switch" and toggles with Space', async () => {
    render(<Switch aria-label="Check for updates" />);
    const sw = screen.getByRole('switch', { name: 'Check for updates' });
    expect(sw).not.toBeChecked();
    sw.focus();
    await userEvent.keyboard(' ');
    expect(sw).toBeChecked();
  });

  it('does not toggle when disabled', async () => {
    render(<Switch aria-label="Locked" disabled />);
    await userEvent.click(screen.getByRole('switch'));
    expect(screen.getByRole('switch')).not.toBeChecked();
  });
});
