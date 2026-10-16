import { render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';

import { ErrorBoundary } from './ErrorBoundary';

function Boom(): never {
  throw new RangeError('Invalid time value');
}

describe('ErrorBoundary', () => {
  afterEach(() => vi.restoreAllMocks());

  it('shows a localized recovery screen instead of a blank app', () => {
    vi.spyOn(console, 'error').mockImplementation(() => {});
    render(
      <I18nProvider locale="es">
        <ErrorBoundary>
          <Boom />
        </ErrorBoundary>
      </I18nProvider>,
    );
    expect(screen.getByRole('alert')).toHaveTextContent(
      'Algo ha fallado en la aplicación.',
    );
    expect(
      screen.getByRole('button', { name: 'Recargar' }),
    ).toBeInTheDocument();
  });

  it('renders its children when nothing throws', () => {
    render(
      <I18nProvider locale="en">
        <ErrorBoundary>
          <p>fine</p>
        </ErrorBoundary>
      </I18nProvider>,
    );
    expect(screen.getByText('fine')).toBeInTheDocument();
  });
});
