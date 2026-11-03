import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { ReactNode } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { LocaleContext, type Locale } from '@/i18n/core';
import type { IntegrationProblem, IntegrationState } from '@/ipc/types';
import { resetNavStore, useNavStore } from '@/stores/navStore';

import { IntegrationLink } from './IntegrationLink';
import { IntegrationNotice } from './IntegrationNotice';
import { IntegrationStatusTag } from './IntegrationStatusTag';

function wrap(children: ReactNode, locale: Locale = 'es') {
  return (
    <LocaleContext.Provider value={locale}>{children}</LocaleContext.Provider>
  );
}

beforeEach(() => resetNavStore());

describe('IntegrationStatusTag', () => {
  const STATES: [IntegrationState, string, boolean][] = [
    ['off', 'Desactivado', true],
    ['unavailable', 'No disponible', true],
    ['not-configured', 'Sin configurar', true],
    ['saved', 'Guardada', false],
    ['connected', 'Conectado', false],
    ['error', 'Error', false],
    ['quota', 'Cuota agotada', false],
  ];

  it.each(STATES)('%s says «%s» with an icon', (state, word, dashed) => {
    const { container } = render(wrap(<IntegrationStatusTag state={state} />));
    const tag = container.firstElementChild!;
    expect(tag).toHaveTextContent(word);
    expect(tag.textContent!.trim()).not.toBe('');
    expect(tag.querySelector('svg[aria-hidden]')).not.toBeNull();
    expect(tag.classList.contains('border-dashed')).toBe(dashed);
  });

  it('every state has its own word', () => {
    const words = STATES.map(([state]) => {
      const { container, unmount } = render(
        wrap(<IntegrationStatusTag state={state} />),
      );
      const text = container.textContent;
      unmount();
      return text;
    });
    expect(new Set(words).size).toBe(STATES.length);
  });

  it('a catalogue state shows its own word', () => {
    render(
      wrap(
        <IntegrationStatusTag
          state={{ state: 'saved', label: 'downloading' }}
        />,
      ),
    );
    expect(screen.getByText('Descargando')).toBeInTheDocument();
    render(
      wrap(
        <IntegrationStatusTag state={{ state: 'connected', label: 'ready' }} />,
      ),
    );
    expect(screen.getByText('Listo')).toBeInTheDocument();
  });

  it('an explicit label wins', () => {
    render(wrap(<IntegrationStatusTag state="saved" label="Guardado" />));
    expect(screen.getByText('Guardado')).toBeInTheDocument();
  });

  it('speaks English too', () => {
    render(wrap(<IntegrationStatusTag state="not-configured" />, 'en'));
    expect(screen.getByText('Not set up')).toBeInTheDocument();
  });
});

describe('IntegrationLink', () => {
  it('opens Settings on the card it names', async () => {
    const open = vi.spyOn(useNavStore.getState(), 'openSettings');
    render(wrap(<IntegrationLink id="steamgriddb" />));
    const link = screen.getByRole('button', { name: 'Configurar' });
    expect(link).toHaveAttribute('data-settings-opener');
    await userEvent.click(link);
    expect(open).toHaveBeenCalledWith('integrations', {
      focus: 'integration-steamgriddb',
    });
    open.mockRestore();
  });

  it('takes its own words and a button look', () => {
    render(
      wrap(
        <IntegrationLink id="screenscraper" variant="button">
          Configurar en Integraciones
        </IntegrationLink>,
      ),
    );
    expect(
      screen.getByRole('button', { name: 'Configurar en Integraciones' }),
    ).toBeInTheDocument();
  });

  it('renders nothing for an unknown or planned service', () => {
    const { container } = render(
      wrap(
        <>
          <IntegrationLink id="nope" />
          <IntegrationLink id="igdb" />
        </>,
      ),
    );
    expect(container).toBeEmptyDOMElement();
  });
});

describe('IntegrationNotice', () => {
  const CASES: [string, IntegrationProblem, string][] = [
    ['steamgriddb', 'missing', 'SteamGridDB sin clave'],
    ['screenscraper', 'missing', 'ScreenScraper sin cuenta'],
    ['steamgriddb', 'rejected', 'SteamGridDB rechazó las credenciales'],
    ['screenscraper', 'quota', 'Cuota de ScreenScraper agotada por hoy'],
    ['steamgriddb', 'off', 'SteamGridDB desactivado'],
    ['switch-catalog', 'off', 'Catálogo de Switch desactivado'],
  ];

  it.each(CASES)(
    '%s %s says «%s» and links to its card',
    (id, problem, text) => {
      render(wrap(<IntegrationNotice notice={{ integration: id, problem }} />));
      const note = screen.getByRole('note');
      expect(note).toHaveAttribute('data-variant', 'warning');
      expect(note).toHaveTextContent(text);
      const link = screen.getByRole('button', { name: 'Configurar' });
      expect(note).toContainElement(link);
      expect(link).toHaveAccessibleDescription(text);
    },
  );

  it('speaks English too', () => {
    render(
      wrap(
        <IntegrationNotice
          notice={{ integration: 'screenscraper', problem: 'missing' }}
        />,
        'en',
      ),
    );
    expect(screen.getByRole('note')).toHaveTextContent(
      'ScreenScraper has no account',
    );
    expect(screen.getByRole('button', { name: 'Set up' })).toBeInTheDocument();
  });

  it('renders nothing for an unknown service', () => {
    const { container } = render(
      wrap(
        <IntegrationNotice notice={{ integration: 'nope', problem: 'off' }} />,
      ),
    );
    expect(container).toBeEmptyDOMElement();
  });
});
