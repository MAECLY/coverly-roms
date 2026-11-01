import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useRef, useState, type RefObject } from 'react';
import { describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';
import type { Locale } from '@/i18n/core';
import type { LargeTransferDetail } from '@/ipc/types';

import { LargeTransferDialog } from './LargeTransferDialog';

const COPY: LargeTransferDetail = {
  bytes: 187_420_000_000,
  items: 3,
  copies: true,
  free: 412_100_000_000,
  volume: 'TinyExt',
};

function Harness({
  detail,
  onCancel,
  onConfirm,
}: {
  detail: LargeTransferDetail;
  onCancel(): void;
  onConfirm(bytes: number): void;
}) {
  const apply = useRef<HTMLButtonElement>(null);
  return (
    <>
      <button ref={apply} type="button">
        Organizar
      </button>
      <LargeTransferDialog
        detail={detail}
        returnFocus={apply}
        onCancel={onCancel}
        onConfirm={onConfirm}
      />
    </>
  );
}

function renderDialog(
  detail: LargeTransferDetail = COPY,
  locale: Locale = 'es',
) {
  const onCancel = vi.fn();
  const onConfirm = vi.fn();
  const r = render(
    <I18nProvider locale={locale}>
      <Harness detail={detail} onCancel={onCancel} onConfirm={onConfirm} />
    </I18nProvider>,
  );
  return { ...r, onCancel, onConfirm };
}

describe('LargeTransferDialog', () => {
  it('asks with the exact size in the title and the button (es)', () => {
    renderDialog();
    const dialog = screen.getByRole('alertdialog', {
      name: '¿Copiar 187,42 GB?',
    });
    expect(
      within(dialog).getByRole('button', { name: 'Copiar 187,42 GB' }),
    ).toBeInTheDocument();
    expect(dialog).toHaveAccessibleDescription(
      'Organizar va a escribir 3 elementos en TinyExt. Ahora hay 412,1 GB libres en ese disco.',
    );
    // Mono only for sizes: the volume is plain text.
    const mono = [...dialog.querySelectorAll('.mono')].map(
      (n) => n.textContent,
    );
    expect(mono).toEqual(['187,42 GB', '412,1 GB', '187,42 GB']);
  });

  it('formats the size for English', () => {
    renderDialog(COPY, 'en');
    const dialog = screen.getByRole('alertdialog', { name: 'Copy 187.42 GB?' });
    expect(
      within(dialog).getByRole('button', { name: 'Copy 187.42 GB' }),
    ).toBeInTheDocument();
    expect(
      within(dialog).getByRole('button', { name: 'Cancel' }),
    ).toBeInTheDocument();
    // The feature keeps its English name, «Organise», as everywhere else.
    expect(dialog).toHaveAccessibleDescription(
      'Organise will write 3 items to TinyExt. 412.1 GB is free on that drive now.',
    );
  });

  it('names a move to another drive as a move', () => {
    renderDialog({ ...COPY, copies: false });
    const dialog = screen.getByRole('alertdialog', {
      name: '¿Mover 187,42 GB a otro disco?',
    });
    expect(
      within(dialog).getByRole('button', { name: 'Mover 187,42 GB' }),
    ).toBeInTheDocument();
  });

  it('says one item in the singular and drops unknown free space', () => {
    const { unmount } = renderDialog({ ...COPY, items: 1 });
    expect(screen.getByRole('alertdialog')).toHaveAccessibleDescription(
      'Organizar va a escribir 1 elemento en TinyExt. Ahora hay 412,1 GB libres en ese disco.',
    );
    unmount();
    const again = renderDialog({ ...COPY, free: null });
    expect(screen.getByRole('alertdialog')).toHaveAccessibleDescription(
      'Organizar va a escribir 3 elementos en TinyExt.',
    );
    again.unmount();
    renderDialog({ ...COPY, items: 1, free: null });
    expect(screen.getByRole('alertdialog')).toHaveAccessibleDescription(
      'Organizar va a escribir 1 elemento en TinyExt.',
    );
  });

  it('starts on Cancelar; Escape and Cancelar cancel without confirming', async () => {
    const user = userEvent.setup();
    const first = renderDialog();
    expect(screen.getByRole('button', { name: 'Cancelar' })).toHaveFocus();
    await user.keyboard('{Escape}');
    expect(first.onCancel).toHaveBeenCalledTimes(1);
    expect(first.onConfirm).not.toHaveBeenCalled();
    first.unmount();

    const second = renderDialog();
    await user.click(screen.getByRole('button', { name: 'Cancelar' }));
    expect(second.onCancel).toHaveBeenCalledTimes(1);
    expect(second.onConfirm).not.toHaveBeenCalled();
  });

  it('confirms with the bytes it showed, and only confirms', async () => {
    const user = userEvent.setup();
    const { onCancel, onConfirm } = renderDialog();
    await user.click(screen.getByRole('button', { name: 'Copiar 187,42 GB' }));
    expect(onConfirm).toHaveBeenCalledTimes(1);
    expect(onConfirm).toHaveBeenCalledWith(187_420_000_000);
    expect(onCancel).not.toHaveBeenCalled();
  });

  it('gives focus back to the apply button when it closes', async () => {
    const user = userEvent.setup();
    const onCancel = vi.fn();
    function Host() {
      const apply = useRef<HTMLButtonElement>(null);
      return (
        <>
          <button ref={apply} type="button">
            Organizar
          </button>
          <Closable apply={apply} />
        </>
      );
    }
    function Closable({
      apply,
    }: {
      apply: RefObject<HTMLButtonElement | null>;
    }) {
      const [open, setOpen] = useState(true);
      return open ? (
        <LargeTransferDialog
          detail={COPY}
          returnFocus={apply}
          onCancel={() => {
            onCancel();
            setOpen(false);
          }}
          onConfirm={() => setOpen(false)}
        />
      ) : null;
    }
    render(
      <I18nProvider locale="es">
        <Host />
      </I18nProvider>,
    );
    await user.keyboard('{Escape}');
    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Organizar' })).toHaveFocus();
  });
});
