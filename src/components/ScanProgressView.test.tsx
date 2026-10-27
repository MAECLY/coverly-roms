import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { ScanProgressView } from '@/components/ScanProgressView';
import { I18nProvider } from '@/i18n/I18nProvider';

describe('ScanProgressView', () => {
  it('formats the ROMs found so far for the locale', () => {
    render(
      <I18nProvider locale="es">
        <ScanProgressView
          path="/mock/Fixture SD (12k)"
          progress={{
            generation: 1,
            done: 3,
            total: 18,
            folder: 'SFC',
            roms: 12345,
          }}
        />
      </I18nProvider>,
    );
    expect(screen.getByText('12.345 ROMs encontradas')).toBeInTheDocument();
    const bar = screen.getByRole('progressbar');
    expect(bar).toHaveAttribute('aria-valuenow', '3');
    expect(bar).toHaveAttribute('aria-valuemax', '18');
  });

  it('announces only the sentence, not every tick (AP-3)', () => {
    render(
      <I18nProvider locale="es">
        <ScanProgressView
          path="/mock/Fixture SD (12k)"
          progress={{
            generation: 1,
            done: 12,
            total: 40,
            folder: 'Game Boy Advance',
            roms: 312,
          }}
        />
      </I18nProvider>,
    );
    expect(screen.getByRole('status').textContent).toBe('Leyendo la carpeta…');
  });

  it('reads out the folder count in mono, the ROMs found and the folder', () => {
    render(
      <I18nProvider locale="es">
        <ScanProgressView
          path="/mock/Fixture SD (12k)"
          progress={{
            generation: 1,
            done: 12,
            total: 40,
            folder: 'Game Boy Advance',
            roms: 312,
          }}
        />
      </I18nProvider>,
    );
    const amount = screen.getByText(
      (_, el) =>
        el?.tagName === 'SPAN' && el.textContent === 'Carpeta 12 de 40',
    );
    const mono = [...amount.querySelectorAll('.mono')].map(
      (e) => e.textContent,
    );
    expect(mono).toEqual(['12', '40']);
    expect(screen.getByText('312 ROMs encontradas')).toBeInTheDocument();
    const folder = screen.getByText('Game Boy Advance');
    expect(folder.closest('.mono')).not.toBeNull();
    expect(folder.closest('[title]')).toHaveAttribute(
      'title',
      'Game Boy Advance',
    );
    expect(
      screen.getByRole('img', { name: 'Progreso del escaneo' }),
    ).toBeInTheDocument();
  });

  it('is indeterminate until the first folder reports', () => {
    render(
      <I18nProvider locale="en">
        <ScanProgressView path={null} progress={null} />
      </I18nProvider>,
    );
    expect(screen.getByRole('progressbar')).not.toHaveAttribute(
      'aria-valuenow',
    );
    expect(screen.getByRole('status').textContent).toBe('Reading the folder…');
  });
});
