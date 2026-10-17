import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import {
  NativeSelect,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from './select';
import { Slider } from './slider';

describe('NativeSelect', () => {
  it('stays a native combobox that userEvent.selectOptions drives', async () => {
    render(
      <NativeSelect aria-label="Log level" defaultValue="info">
        <option value="info">info</option>
        <option value="debug">debug</option>
      </NativeSelect>,
    );
    const select = screen.getByRole('combobox', { name: 'Log level' });
    await userEvent.selectOptions(select, 'debug');
    expect(select).toHaveValue('debug');
  });
});

describe('Select (Radix)', () => {
  it('opens from the keyboard and picks an option', async () => {
    const onValueChange = vi.fn();
    render(
      <Select onValueChange={onValueChange}>
        <SelectTrigger aria-label="Language">
          <SelectValue placeholder="Pick" />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="en">English</SelectItem>
          <SelectItem value="es">Español</SelectItem>
        </SelectContent>
      </Select>,
    );
    const trigger = screen.getByRole('combobox', { name: 'Language' });
    expect(trigger).toHaveAttribute('aria-expanded', 'false');
    trigger.focus();
    await userEvent.keyboard('{Enter}');
    expect(trigger).toHaveAttribute('aria-expanded', 'true');
    await userEvent.click(screen.getByRole('option', { name: 'Español' }));
    expect(onValueChange).toHaveBeenCalledWith('es');
    expect(trigger).toHaveTextContent('Español');
  });
});

describe('Slider', () => {
  it('names the thumb and steps with the arrow keys', async () => {
    const onValueChange = vi.fn();
    render(
      <Slider
        defaultValue={[50]}
        min={0}
        max={100}
        step={5}
        thumbLabel="Minimum match"
        onValueChange={onValueChange}
      />,
    );
    const thumb = screen.getByRole('slider', { name: 'Minimum match' });
    expect(thumb).toHaveAttribute('aria-valuenow', '50');
    thumb.focus();
    await userEvent.keyboard('{ArrowRight}');
    expect(onValueChange).toHaveBeenLastCalledWith([55]);
    await userEvent.keyboard('{Home}');
    expect(onValueChange).toHaveBeenLastCalledWith([0]);
  });
});
