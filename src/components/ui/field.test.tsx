import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';

import { Checkbox } from './checkbox';
import {
  Field,
  FieldControl,
  FieldDescription,
  FieldError,
  FieldLabel,
} from './field';
import { Input } from './input';
import { Label } from './label';
import { Textarea } from './textarea';

describe('Field', () => {
  it('links label, description and error to the control', () => {
    render(
      <Field>
        <FieldLabel>Proxy URL</FieldLabel>
        <FieldControl>
          <Input />
        </FieldControl>
        <FieldDescription>http or socks5</FieldDescription>
        <FieldError>Not a valid URL</FieldError>
      </Field>,
    );
    const input = screen.getByRole('textbox', { name: 'Proxy URL' });
    expect(input).toHaveAccessibleDescription('http or socks5 Not a valid URL');
    expect(input).toHaveAttribute('aria-invalid', 'true');
    expect(screen.getByRole('alert')).toHaveTextContent('Not a valid URL');
  });

  it('renders nothing and stays valid when the error is empty', () => {
    render(
      <Field id="backup">
        <FieldLabel>Backup folder</FieldLabel>
        <FieldControl>
          <Input mono />
        </FieldControl>
        <FieldError>{null}</FieldError>
      </Field>,
    );
    const input = screen.getByRole('textbox', { name: 'Backup folder' });
    expect(input).toHaveAttribute('id', 'backup');
    expect(input).not.toHaveAttribute('aria-invalid');
    expect(input).not.toHaveAttribute('aria-describedby');
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('wires a Radix control in the horizontal layout', async () => {
    render(
      <Field orientation="horizontal">
        <FieldControl>
          <Checkbox />
        </FieldControl>
        <FieldLabel>Only missing art</FieldLabel>
      </Field>,
    );
    const box = screen.getByRole('checkbox', { name: 'Only missing art' });
    await userEvent.click(screen.getByText('Only missing art'));
    expect(box).toBeChecked();
  });
});

describe('Input, Textarea, Label', () => {
  it('types into an input and a textarea named by Label', async () => {
    render(
      <>
        <Label htmlFor="a">Name</Label>
        <Input id="a" />
        <Label htmlFor="b">Paths</Label>
        <Textarea id="b" mono />
      </>,
    );
    await userEvent.type(screen.getByRole('textbox', { name: 'Name' }), 'gba');
    await userEvent.type(
      screen.getByRole('textbox', { name: 'Paths' }),
      '/roms',
    );
    expect(screen.getByRole('textbox', { name: 'Name' })).toHaveValue('gba');
    expect(screen.getByRole('textbox', { name: 'Paths' })).toHaveValue('/roms');
  });
});
