import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { ReactNode } from 'react';
import { describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui/tooltip';
import { LocaleContext, type Locale } from '@/i18n/core';
import { BackendError } from '@/ipc/errors';
import type { CredentialKey, FieldInput } from '@/ipc/types';

import { SecretField } from './SecretField';

function wrap(children: ReactNode, locale: Locale = 'es') {
  return (
    <LocaleContext.Provider value={locale}>
      <TooltipProvider>{children}</TooltipProvider>
    </LocaleContext.Provider>
  );
}

function setup(
  over: Partial<{
    k: CredentialKey;
    input: FieldInput;
    set: boolean;
    disabled: boolean;
    onSave: (value: string) => Promise<void>;
  }> = {},
) {
  const onSave = over.onSave ?? vi.fn(() => Promise.resolve());
  const props = {
    k: over.k ?? ('steamgriddb-api-key' as CredentialKey),
    input: over.input ?? ('api-key' as FieldInput),
    set: over.set ?? false,
    disabled: over.disabled ?? false,
    onSave,
  };
  const view = render(wrap(<SecretField {...props} />));
  const rerender = (next: Partial<typeof props>) =>
    view.rerender(wrap(<SecretField {...props} {...next} />));
  return { onSave, rerender };
}

const field = () => screen.getByLabelText('Clave de API');
const show = () => screen.getByRole('button', { name: 'Mostrar Clave de API' });
const save = () => screen.getByRole('button', { name: 'Guardar Clave de API' });

describe('SecretField', () => {
  it('is a masked input that browsers and password managers leave alone', () => {
    setup();
    const input = field();
    expect(input).toHaveAttribute('type', 'password');
    expect(input).toHaveAttribute('autocomplete', 'off');
    expect(input).toHaveAttribute('spellcheck', 'false');
    expect(input).toHaveAttribute('name', 'integration-steamgriddb-api-key');
    expect(input).not.toHaveAttribute('name', 'password');
  });

  it('Mostrar flips the typed text visible and says which field it controls', async () => {
    setup();
    const input = field();
    expect(show()).toBeDisabled();
    await userEvent.type(input, 'sk-PEEK');
    expect(show()).toHaveAttribute('aria-pressed', 'false');
    expect(show()).toHaveAttribute('aria-controls', input.id);
    await userEvent.click(show());
    expect(show()).toHaveAttribute('aria-pressed', 'true');
    expect(input).toHaveAttribute('type', 'text');
    await userEvent.click(show());
    expect(input).toHaveAttribute('type', 'password');
  });

  it('emptying the field by hand masks it again for the next value', async () => {
    setup();
    const input = field();
    await userEvent.type(input, 'sk-PEEK');
    await userEvent.click(show());
    expect(input).toHaveAttribute('type', 'text');
    await userEvent.clear(input);
    expect(input).toHaveAttribute('type', 'password');
    expect(show()).toHaveAttribute('aria-pressed', 'false');
    await userEvent.type(input, 'sk-NEXT');
    expect(input).toHaveAttribute('type', 'password');
  });

  it('the value cannot change while it is being saved', async () => {
    let done!: () => void;
    const onSave = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          done = resolve;
        }),
    );
    setup({ onSave });
    await userEvent.type(field(), 'sk-ONE');
    await userEvent.click(save());
    expect(field()).toHaveAttribute('readonly');
    await userEvent.type(field(), 'X');
    expect(field()).toHaveValue('sk-ONE');
    done();
    await screen.findByText('Guardar');
    expect(field()).not.toHaveAttribute('readonly');
    expect(field()).toHaveValue('');
  });

  it('Guardar is disabled while empty or blank', async () => {
    setup();
    expect(save()).toBeDisabled();
    await userEvent.type(field(), '   ');
    expect(save()).toBeDisabled();
    await userEvent.type(field(), 'k');
    expect(save()).toBeEnabled();
  });

  it('a stored value is never put back in the input', () => {
    setup({ set: true });
    expect((field() as HTMLInputElement).value).toBe('');
    expect(screen.getByText('Guardada en el llavero')).toBeInTheDocument();
  });

  it('after a save the input is empty, masked again and says it is stored', async () => {
    const { onSave, rerender } = setup();
    await userEvent.type(field(), 'sk-SECRET');
    await userEvent.click(show());
    await userEvent.click(save());
    expect(onSave).toHaveBeenCalledWith('sk-SECRET');
    expect(field()).toHaveValue('');
    expect(field()).toHaveAttribute('type', 'password');
    rerender({ set: true });
    const saved = screen.getByText('Guardada en el llavero');
    expect(
      saved.closest('p')!.querySelector('svg[aria-hidden]'),
    ).not.toBeNull();
    expect(field()).toHaveAccessibleDescription('Guardada en el llavero');
    expect(document.body.innerHTML).not.toContain('sk-SECRET');
  });

  it('the stored line follows the gender of the field', () => {
    setup({ k: 'screenscraper-user', input: 'user', set: true });
    expect(screen.getByText('Guardado en el llavero')).toBeInTheDocument();
  });

  it('while saving the button says Guardando… and is disabled', async () => {
    let done!: () => void;
    const onSave = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          done = resolve;
        }),
    );
    setup({ onSave });
    await userEvent.type(field(), 'sk-WAIT');
    await userEvent.click(save());
    const busy = screen.getByRole('button', { name: 'Guardar Clave de API' });
    expect(busy).toHaveTextContent('Guardando…');
    expect(busy).toBeDisabled();
    done();
    await screen.findByText('Guardar');
    expect(field()).toHaveValue('');
  });

  it('after a rejected save the input is empty and the error is shown', async () => {
    const onSave = vi.fn(() =>
      Promise.reject(new BackendError('keychain-locked')),
    );
    setup({ onSave });
    await userEvent.type(field(), 'sk-NOPE');
    await userEvent.click(save());
    const alert = await screen.findByRole('alert');
    expect(alert.textContent).not.toBe('');
    expect(field()).toHaveValue('');
    expect(field()).toHaveAttribute('aria-invalid', 'true');
    expect(document.body.innerHTML).not.toContain('sk-NOPE');
  });

  it('a value the keychain refuses says why', async () => {
    const onSave = vi.fn(() =>
      Promise.reject(new BackendError('bad-request', 'credential value')),
    );
    setup({ onSave });
    await userEvent.type(field(), 'sk');
    await userEvent.click(save());
    expect(await screen.findByRole('alert')).toHaveTextContent(
      'La clave no es válida: tiene caracteres de control o es demasiado larga.',
    );
  });

  it('a new value clears the previous error', async () => {
    const onSave = vi
      .fn<(v: string) => Promise<void>>()
      .mockRejectedValueOnce(new BackendError('keychain-locked'));
    setup({ onSave });
    await userEvent.type(field(), 'a');
    await userEvent.click(save());
    await screen.findByRole('alert');
    await userEvent.type(field(), 'b');
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('disabled leaves every control off', () => {
    setup({ disabled: true });
    expect(field()).toBeDisabled();
    expect(save()).toBeDisabled();
  });

  it('speaks English too', () => {
    render(
      wrap(
        <SecretField
          k="screenscraper-password"
          input="password"
          set
          disabled={false}
          onSave={() => Promise.resolve()}
        />,
        'en',
      ),
    );
    expect(screen.getByLabelText('Password')).toHaveAttribute(
      'type',
      'password',
    );
    expect(screen.getByText('Saved in the keychain')).toBeInTheDocument();
  });
});
