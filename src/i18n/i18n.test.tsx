import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { pickLocale } from './core';
import { I18nProvider } from './I18nProvider';
import { useT } from './useT';

function Probe({ k, vars }: { k: string; vars?: Record<string, number> }) {
  const t = useT();
  return <p>{t(k, vars)}</p>;
}

describe('i18n', () => {
  it('renders the Spanish tagline', () => {
    render(
      <I18nProvider locale="es">
        <Probe k="app.tagline" />
      </I18nProvider>,
    );
    expect(
      screen.getByText('Encuentra y añade las portadas que faltan en tus ROMs'),
    ).toBeInTheDocument();
  });

  it('renders the English tagline', () => {
    render(
      <I18nProvider locale="en">
        <Probe k="app.tagline" />
      </I18nProvider>,
    );
    expect(
      screen.getByText('Find and add the missing cover art in your ROMs'),
    ).toBeInTheDocument();
  });

  it('interpolates variables', () => {
    render(
      <I18nProvider locale="en">
        <Probe k="library.count" vars={{ n: 3 }} />
      </I18nProvider>,
    );
    expect(screen.getByText('3 ROMs')).toBeInTheDocument();
  });

  it('falls back to the key when it is missing everywhere', () => {
    render(
      <I18nProvider locale="es">
        <Probe k="does.not.exist" />
      </I18nProvider>,
    );
    expect(screen.getByText('does.not.exist')).toBeInTheDocument();
  });

  it('picks Spanish for any es-* language and English otherwise', () => {
    expect(pickLocale('es-ES')).toBe('es');
    expect(pickLocale('es')).toBe('es');
    expect(pickLocale('ES-mx')).toBe('es');
    expect(pickLocale('en-US')).toBe('en');
    expect(pickLocale('fr-FR')).toBe('en');
    expect(pickLocale(undefined)).toBe('en');
  });
});
