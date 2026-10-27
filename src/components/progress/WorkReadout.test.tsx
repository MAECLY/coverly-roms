import { render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';

import { WorkReadout, type WorkReadoutProps } from './WorkReadout';

const AMOUNT = '312,4 MB de 591,9 MB';

const BASE: WorkReadoutProps = {
  bar: { value: 300, max: 1200, slots: 16 },
  label: 'Progreso de la extracción',
  amount: AMOUNT,
  amountText: AMOUNT,
  rate: '24,1 MB/s',
  eta: null,
  counters: ['Juego 2 de 3', 'Pista 18 de 52'],
  action: <button type="button">Cancelar</button>,
};

/** Renders in `es` with `props` over the base, minus the `omit` keys. */
function renderEs(
  props: Partial<WorkReadoutProps> = {},
  omit: (keyof WorkReadoutProps)[] = [],
) {
  const all: Partial<WorkReadoutProps> = { ...BASE, ...props };
  for (const k of omit) delete all[k];
  return render(
    <I18nProvider locale="es">
      <WorkReadout {...(all as WorkReadoutProps)} />
    </I18nProvider>,
  );
}

describe('WorkReadout', () => {
  it('gives the bar the amount as its value text', () => {
    renderEs();
    const bar = screen.getByRole('progressbar', {
      name: 'Progreso de la extracción',
    });
    expect(bar).toHaveAttribute('aria-valuetext', AMOUNT);
    expect(bar).toHaveAttribute('aria-valuenow', '300');
    expect(bar).toHaveAttribute('aria-valuemax', '1200');
  });

  it('says it is measuring while the time left is unknown', () => {
    renderEs();
    expect(screen.getByText('Calculando el tiempo…')).toBeInTheDocument();
  });

  it('shows the time left when given', () => {
    renderEs({ eta: 'Quedan unos 12 s' });
    expect(screen.getByText('Quedan unos 12 s')).toBeInTheDocument();
    expect(screen.queryByText('Calculando el tiempo…')).toBeNull();
  });

  it('shows no time text when the time left does not apply', () => {
    renderEs({}, ['eta']);
    expect(screen.queryByText('Calculando el tiempo…')).toBeNull();
    expect(screen.getByText('24,1 MB/s')).toBeInTheDocument();
  });

  it('shows the counters, the rate and the action', () => {
    renderEs();
    expect(screen.getByText('Juego 2 de 3')).toBeInTheDocument();
    expect(screen.getByText('Pista 18 de 52')).toBeInTheDocument();
    expect(screen.getByText('24,1 MB/s')).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Cancelar' }),
    ).toBeInTheDocument();
  });

  it('keeps the readout row out of live announcements', () => {
    renderEs();
    const row = screen.getByText(AMOUNT).closest('[aria-live]');
    expect(row).toHaveAttribute('aria-live', 'off');
    expect(screen.getByText('Juego 2 de 3').closest('[aria-live]')).toBe(row);
  });

  it('sets the numbers of the amount in tabular figures', () => {
    renderEs();
    expect(screen.getByText(AMOUNT).closest('.tabular-nums')).not.toBeNull();
  });

  it('leaves the bar to Radix value text when no amount text is given', () => {
    renderEs({}, ['amountText']);
    const bar = screen.getByRole('progressbar');
    expect(bar.getAttribute('aria-valuetext')).not.toBe(AMOUNT);
  });

  it('draws no counters row and no action when there are none', () => {
    const { container } = renderEs({ counters: [] }, ['action']);
    expect(screen.queryByRole('button')).toBeNull();
    expect(container.querySelectorAll('[aria-live="off"] > p')).toHaveLength(1);
  });

  describe('bar bounds', () => {
    afterEach(() => vi.restoreAllMocks());

    it('keeps the bar full, not indeterminate, when the value overshoots', () => {
      const error = vi.spyOn(console, 'error').mockImplementation(() => {});
      renderEs({ bar: { value: 1300, max: 1200, slots: 16 } });
      const bar = screen.getByRole('progressbar');
      expect(bar).toHaveAttribute('aria-valuenow', '1200');
      expect(bar).toHaveAttribute('aria-valuetext', AMOUNT);
      expect(error).not.toHaveBeenCalled();
    });

    it('reads a negative value as nothing done', () => {
      const error = vi.spyOn(console, 'error').mockImplementation(() => {});
      renderEs({ bar: { value: -5, max: 1200 } });
      expect(screen.getByRole('progressbar')).toHaveAttribute(
        'aria-valuenow',
        '0',
      );
      expect(error).not.toHaveBeenCalled();
    });

    it('shows an indeterminate bar when there is nothing to count yet', () => {
      const error = vi.spyOn(console, 'error').mockImplementation(() => {});
      renderEs({ bar: { value: 0, max: 0, slots: 16 } });
      const bar = screen.getByRole('progressbar');
      expect(bar).not.toHaveAttribute('aria-valuenow');
      expect(error).not.toHaveBeenCalled();
    });
  });

  it('draws no empty amount line when only counters are given', () => {
    const { container } = renderEs({ counters: ['312 ROMs encontradas'] }, [
      'amount',
      'amountText',
      'rate',
      'eta',
    ]);
    const ps = container.querySelectorAll('[aria-live="off"] > p');
    expect(ps).toHaveLength(1);
    expect(ps[0]).toHaveTextContent('312 ROMs encontradas');
  });
});
