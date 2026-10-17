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

  it('is indeterminate until the first folder reports', () => {
    render(
      <I18nProvider locale="en">
        <ScanProgressView path={null} progress={null} />
      </I18nProvider>,
    );
    expect(screen.getByRole('progressbar')).not.toHaveAttribute(
      'aria-valuenow',
    );
  });
});
