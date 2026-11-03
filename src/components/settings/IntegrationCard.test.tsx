import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState, type ReactNode } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui/tooltip';
import { LocaleContext, type Locale } from '@/i18n/core';
import { setBackend } from '@/ipc/backend';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import {
  createMockBackend,
  type MockBackend,
  type MockOptions,
} from '@/ipc/mock/mockBackend';
import type {
  IntegrationTestResult,
  IntegrationView,
  KeyCheck,
} from '@/ipc/types';
import { integration } from '@/lib/integrations';
import { resetNavStore, useNavStore } from '@/stores/navStore';
import { resetSettingsStore, useSettingsStore } from '@/stores/settingsStore';
import { MemoryStorage } from '@/test/storage';

import { IntegrationCard, resetIntegrationCardMemory } from './IntegrationCard';

let backend: MockBackend;

function wrap(children: ReactNode, locale: Locale) {
  return (
    <LocaleContext.Provider value={locale}>
      <TooltipProvider>{children}</TooltipProvider>
    </LocaleContext.Provider>
  );
}

/** The card with its views kept like the tab keeps them. */
function Harness({
  id,
  first,
  over,
  platform,
}: {
  id: string;
  first: IntegrationView[];
  over?: Partial<IntegrationView> | undefined;
  platform: string;
}) {
  const [views, setViews] = useState(first);
  const view = { ...views.find((v) => v.id === id)!, ...over };
  return (
    <div data-testid="parent">
      <IntegrationCard
        entry={integration(id)!}
        view={view}
        order={{ n: 2, total: 4 }}
        platform={platform}
        onView={setViews}
      />
    </div>
  );
}

async function setup(
  id: string,
  opts: {
    locale?: Locale;
    over?: Partial<IntegrationView>;
    backend?: Partial<MockOptions>;
    enable?: string[];
    platform?: string;
  } = {},
) {
  backend = createMockBackend({
    delayMs: 0,
    now: FIXTURE_NOW,
    storage: new MemoryStorage(),
    integrations: null,
    keychain: null,
    ...opts.backend,
  });
  setBackend(backend);
  if (opts.enable?.length) {
    const s = await backend.getSettings();
    await backend.updateSettings({
      sources: {
        order: s.settings.sources.order.map((e) =>
          opts.enable!.includes(e.id) ? { ...e, enabled: true } : e,
        ),
      },
    });
  }
  await act(() => useSettingsStore.getState().load());
  const first = await backend.integrationsView();
  const mount = () =>
    render(
      wrap(
        <Harness
          id={id}
          first={first}
          over={opts.over}
          platform={opts.platform ?? 'macos'}
        />,
        opts.locale ?? 'es',
      ),
    );
  return { first, remount: () => mount(), view: mount() };
}

const header = (name: RegExp | string) => screen.getByRole('button', { name });
const body = (id: string) => document.getElementById(`integration-${id}-body`)!;
const live = (id: string) =>
  within(body(id)).getByRole('status', { hidden: true });

async function openCard(id: string) {
  act(() => useNavStore.getState().setFold(`integration-${id}`, true));
  await waitFor(() => expect(body(id)).not.toHaveAttribute('hidden'));
}

describe('IntegrationCard', () => {
  beforeEach(() => {
    resetSettingsStore();
    resetNavStore();
    resetIntegrationCardMemory();
  });
  afterEach(() => {
    vi.restoreAllMocks();
    setBackend(null);
  });

  it('header: an h3 button named «service, state» that folds a mounted body', async () => {
    await setup('steamgriddb', { enable: ['steamgriddb'] });
    const button = header('SteamGridDB, Sin configurar');
    expect(button.closest('h3')).not.toBeNull();
    expect(button).toHaveAttribute('id', 'integration-steamgriddb');
    expect(button).toHaveAttribute('aria-expanded', 'false');
    expect(button).toHaveAttribute(
      'aria-controls',
      'integration-steamgriddb-body',
    );
    expect(button).toHaveAttribute(
      'aria-describedby',
      'integration-steamgriddb-use',
    );
    expect(
      document.getElementById('integration-steamgriddb-use'),
    ).toHaveTextContent('Portadas hechas por la comunidad');
    const article = button.closest('article')!;
    expect(article).toHaveAttribute(
      'data-advanced-fold',
      'integration-steamgriddb',
    );
    expect(article).toHaveAttribute('data-setting-row');
    // Closed: mounted (search and deep links need it), hidden, untabbable.
    expect(body('steamgriddb')).toBeInTheDocument();
    expect(body('steamgriddb')).toHaveAttribute('hidden');
    expect(within(body('steamgriddb')).queryAllByRole('button')).toHaveLength(
      0,
    );
    await userEvent.click(button);
    expect(button).toHaveAttribute('aria-expanded', 'true');
    expect(body('steamgriddb')).not.toHaveAttribute('hidden');
    expect(useNavStore.getState().openFolds).toContain(
      'integration-steamgriddb',
    );
    await userEvent.click(button);
    expect(button).toHaveAttribute('aria-expanded', 'false');
  });

  it('saving a key refreshes the card to stored and never keeps the value', async () => {
    await setup('steamgriddb', { enable: ['steamgriddb'] });
    await openCard('steamgriddb');
    await userEvent.type(
      screen.getByLabelText('Clave de API'),
      'sgdb-secret-1234',
    );
    await userEvent.click(
      screen.getByRole('button', { name: 'Guardar Clave de API' }),
    );
    await waitFor(() =>
      expect(header('SteamGridDB, Guardada')).toBeInTheDocument(),
    );
    expect(screen.getByLabelText('Clave de API')).toHaveValue('');
    expect(document.body.innerHTML).not.toContain('sgdb-secret-1234');
    expect(screen.getByText('Sin comprobar en esta sesión.')).toBeVisible();
  });

  it('«Probar conexión» says each outcome in a live region mounted beforehand', async () => {
    await setup('steamgriddb', { enable: ['steamgriddb'] });
    await backend.setCredential('steamgriddb-api-key', 'good');
    await openCard('steamgriddb');
    const region = live('steamgriddb');
    expect(region).toBeEmptyDOMElement();
    const copy: Record<KeyCheck, string> = {
      ok: 'Funciona.',
      invalid: 'Credenciales incorrectas.',
      missing: 'Falta la clave.',
      offline: 'Sin red.',
      'rate-limited': 'Cuota agotada, demasiadas peticiones',
      unavailable: 'El servicio no responde.',
    };
    const spy = vi.spyOn(backend, 'testIntegration');
    // The view says a key is stored only after a refresh: save through
    // the card so the test button is enabled.
    await userEvent.type(screen.getByLabelText('Clave de API'), 'good');
    await userEvent.click(
      screen.getByRole('button', { name: 'Guardar Clave de API' }),
    );
    const test = await screen.findByRole('button', {
      name: 'Probar conexión',
    });
    await waitFor(() => expect(test).toBeEnabled());
    for (const verdict of Object.keys(copy) as KeyCheck[]) {
      spy.mockResolvedValueOnce({ verdict });
      await userEvent.click(test);
      await waitFor(() =>
        expect(live('steamgriddb')).toHaveTextContent(copy[verdict]),
      );
      expect(live('steamgriddb')).toBe(region);
    }
    expect(spy).toHaveBeenCalledWith('steamgriddb');
  });

  it('a test answer that arrives after a save is dropped; «Probando…» meanwhile', async () => {
    await setup('steamgriddb', { enable: ['steamgriddb'] });
    await backend.setCredential('steamgriddb-api-key', 'good');
    useNavStore.getState().setFold('integration-steamgriddb', true);
    // Re-render with the stored key through a save.
    await waitFor(() =>
      expect(body('steamgriddb')).not.toHaveAttribute('hidden'),
    );
    await userEvent.type(screen.getByLabelText('Clave de API'), 'good');
    await userEvent.click(
      screen.getByRole('button', { name: 'Guardar Clave de API' }),
    );
    let answer!: (r: IntegrationTestResult) => void;
    vi.spyOn(backend, 'testIntegration').mockImplementationOnce(
      () => new Promise((r) => (answer = r)),
    );
    const test = await screen.findByRole('button', {
      name: 'Probar conexión',
    });
    await waitFor(() => expect(test).toBeEnabled());
    await userEvent.click(test);
    expect(test).toHaveTextContent('Probando…');
    expect(test).toBeDisabled();
    expect(test.querySelector('[aria-hidden] svg rect')).not.toBeNull();
    await userEvent.type(screen.getByLabelText('Clave de API'), 'other');
    await userEvent.click(
      screen.getByRole('button', { name: 'Guardar Clave de API' }),
    );
    await act(async () => answer({ verdict: 'invalid' }));
    // The region says the new save, never the old key's verdict.
    expect(live('steamgriddb')).toHaveTextContent(
      'Clave de API: se ha guardado en el llavero.',
    );
    expect(live('steamgriddb')).not.toHaveTextContent('Credenciales');
    expect(
      screen.getByRole('button', { name: 'Probar conexión' }),
    ).toHaveTextContent('Probar conexión');
  });

  it('ScreenScraper without developer credentials: fields editable, test disabled with the reason', async () => {
    await setup('screenscraper');
    expect(header('ScreenScraper, No disponible')).toBeInTheDocument();
    await openCard('screenscraper');
    expect(screen.getByLabelText('Usuario')).toBeEnabled();
    expect(screen.getByLabelText('Contraseña')).toBeEnabled();
    const test = screen.getByRole('button', { name: 'Probar conexión' });
    expect(test).toBeDisabled();
    expect(test).toHaveAccessibleDescription(
      'Esta versión aún no incluye acceso a ScreenScraper. Puedes guardar tu cuenta para cuando llegue.',
    );
  });

  it('quota: numbers in mono, formatted per locale, with when it was seen', async () => {
    vi.spyOn(Date, 'now').mockReturnValue(1_800_000_120_000);
    const quota = {
      requests_today: 1234,
      max_requests_per_day: 20_000,
      max_threads: 1,
    };
    await setup('screenscraper', {
      backend: { integrations: 'dev' },
      over: { quota, quota_at: 1_800_000_000 },
    });
    await openCard('screenscraper');
    const line = screen.getByTestId('integration-screenscraper-quota');
    expect(line).toHaveTextContent('Hoy: 1234 de 20.000 peticiones');
    expect(line).toHaveTextContent('Hilos: 1');
    // Its own item that wraps to a line of its own: sentence case.
    expect(within(line).getByText('Visto hace 2 minutos')).toBeInTheDocument();
    expect(within(line).getByText('20.000').closest('.mono')).not.toBeNull();
    expect(within(line).getByText('1234').closest('.mono')).not.toBeNull();
    expect(within(line).getByText('1').closest('.mono')).not.toBeNull();
  });

  it('quota in English uses its own grouping', async () => {
    await setup('screenscraper', {
      locale: 'en',
      backend: { integrations: 'dev' },
      over: {
        quota: {
          requests_today: 1234,
          max_requests_per_day: 20_000,
          max_threads: 1,
        },
      },
    });
    await openCard('screenscraper');
    expect(
      screen.getByTestId('integration-screenscraper-quota'),
    ).toHaveTextContent('Today: 1,234 of 20,000 requests');
  });

  it('a used-up quota a job saw without numbers still says why and what to do', async () => {
    // A fill that hit «quota exhausted» (HTTP 430) carries no numbers, or
    // only older ones from before it ran out: the line must not be blank
    // nor claim a count that is not used up.
    for (const quota of [
      undefined,
      { requests_today: 100, max_requests_per_day: 20_000, max_threads: 1 },
    ]) {
      const { view } = await setup('screenscraper', {
        backend: { integrations: 'dev' },
        over: { state: 'quota', quota, quota_at: 1_800_000_000 },
      });
      await openCard('screenscraper');
      const line = document.getElementById('integration-screenscraper-state');
      expect(line).toHaveTextContent(
        'Cuota diaria agotada. Vuelve mañana; mientras, el relleno sigue con las demás fuentes.',
      );
      expect(line).not.toHaveTextContent('Has usado');
      view.unmount();
      resetNavStore();
    }
  });

  it('a used-up quota with its numbers names them', async () => {
    await setup('screenscraper', {
      backend: { integrations: 'dev' },
      over: {
        state: 'quota',
        quota: {
          requests_today: 20_000,
          max_requests_per_day: 20_000,
          max_threads: 1,
        },
      },
    });
    await openCard('screenscraper');
    expect(
      document.getElementById('integration-screenscraper-state'),
    ).toHaveTextContent('Has usado 20.000 de 20.000 peticiones hoy.');
  });

  it('no quota line without numbers', async () => {
    await setup('screenscraper', { backend: { integrations: 'dev' } });
    await openCard('screenscraper');
    expect(
      screen.queryByTestId('integration-screenscraper-quota'),
    ).not.toBeInTheDocument();
  });

  it('forget: an in-page confirm, Escape stays in the card, confirm forgets', async () => {
    const confirm = vi.spyOn(window, 'confirm');
    await setup('screenscraper', {
      backend: { integrations: 'dev' },
      enable: ['screenscraper'],
    });
    await backend.setCredential('screenscraper-user', 'me');
    await backend.setCredential('screenscraper-password', 'pw');
    await openCard('screenscraper');
    // The stored state arrives with the next view: save through the card.
    await userEvent.type(screen.getByLabelText('Usuario'), 'me');
    await userEvent.click(
      screen.getByRole('button', { name: 'Guardar Usuario' }),
    );
    const forget = await screen.findByRole('button', {
      name: 'Olvidar credenciales',
    });
    await userEvent.click(forget);
    const group = screen.getByRole('group', {
      name: /¿Olvidar las credenciales de ScreenScraper\?/,
    });
    const cancel = within(group).getByRole('button', { name: 'Cancelar' });
    expect(cancel).toHaveFocus();
    expect(
      screen.queryByRole('button', { name: 'Olvidar credenciales' }),
    ).not.toBeInTheDocument();
    const parentKey = vi.fn();
    screen.getByTestId('parent').addEventListener('keydown', parentKey);
    await userEvent.keyboard('{Escape}');
    expect(parentKey).not.toHaveBeenCalled();
    expect(screen.queryByRole('group', { name: /Olvidar/ })).toBeNull();
    expect(
      screen.getByRole('button', { name: 'Olvidar credenciales' }),
    ).toHaveFocus();
    const spy = vi.spyOn(backend, 'forgetIntegration');
    await userEvent.click(
      screen.getByRole('button', { name: 'Olvidar credenciales' }),
    );
    await userEvent.click(screen.getByRole('button', { name: 'Olvidar' }));
    expect(spy).toHaveBeenCalledWith('screenscraper');
    await waitFor(() =>
      expect(screen.getByText('Falta la cuenta.')).toBeInTheDocument(),
    );
    // The mock build keeps ScreenScraper off; nothing is stored any more.
    expect(screen.queryByText('Guardado en el llavero')).toBeNull();
    expect(
      screen.queryByRole('button', { name: 'Olvidar credenciales' }),
    ).toBeNull();
    expect(header(/^ScreenScraper, /)).toHaveFocus();
    expect(confirm).not.toHaveBeenCalled();
  });

  it('the toggle writes sources.order flipping only its own entry', async () => {
    await setup('steamgriddb', {
      over: { fields: [{ key: 'steamgriddb-api-key', set: true }] },
    });
    await openCard('steamgriddb');
    const before = useSettingsStore.getState().settings!.sources.order;
    const toggle = screen.getByRole('switch', { name: 'Activado' });
    expect(toggle).not.toBeChecked();
    expect(toggle).toBeEnabled();
    await userEvent.click(toggle);
    await waitFor(() =>
      expect(useSettingsStore.getState().settings!.sources.order).toEqual(
        before.map((e) =>
          e.id === 'steamgriddb' ? { ...e, enabled: true } : e,
        ),
      ),
    );
    expect(
      screen.getByRole('button', { name: 'Fuentes: posición 2 de 4' }),
    ).toBeInTheDocument();
  });

  it('the toggle waits for a stored key', async () => {
    await setup('steamgriddb');
    await openCard('steamgriddb');
    expect(screen.getByRole('switch', { name: 'Activado' })).toBeDisabled();
    expect(screen.getByText('Falta la clave de API.')).toBeInTheDocument();
  });

  it('turning off the last enabled source shows the refusal next to the switch', async () => {
    await setup('steamgriddb', { enable: ['steamgriddb'] });
    const only = useSettingsStore
      .getState()
      .settings!.sources.order.map((e) => ({
        ...e,
        enabled: e.id === 'steamgriddb',
      }));
    await act(() =>
      useSettingsStore.getState().update({ sources: { order: only } }),
    );
    await openCard('steamgriddb');
    await userEvent.click(screen.getByRole('switch', { name: 'Activado' }));
    const toggle = screen.getByRole('switch', { name: 'Activado' });
    await waitFor(() => expect(toggle).toHaveAccessibleDescription(/./));
    expect(
      document.getElementById('integration-steamgriddb-order-error'),
    ).not.toBeNull();
  });

  it('a keychain error says what to do on this OS', async () => {
    await setup('steamgriddb', {
      enable: ['steamgriddb'],
      backend: { keychain: 'locked' },
      platform: 'macos',
    });
    expect(header('SteamGridDB, Error')).toBeInTheDocument();
    // Error and quota cards start open (D26).
    expect(body('steamgriddb')).not.toHaveAttribute('hidden');
    expect(
      screen.getByText(/El llavero de macOS está bloqueado/),
    ).toBeInTheDocument();
  });

  it('a card the user closed stays closed when it is in error', async () => {
    const { view, remount } = await setup('steamgriddb', {
      enable: ['steamgriddb'],
      backend: { keychain: 'locked' },
    });
    expect(body('steamgriddb')).not.toHaveAttribute('hidden');
    await userEvent.click(header('SteamGridDB, Error'));
    expect(body('steamgriddb')).toHaveAttribute('hidden');
    view.unmount();
    remount();
    expect(body('steamgriddb')).toHaveAttribute('hidden');
  });

  it('the live region stays in the accessibility tree while empty', async () => {
    await setup('steamgriddb', { enable: ['steamgriddb'] });
    await openCard('steamgriddb');
    // display:none would take it out of the tree: a region that appears
    // with its text is not announced (§6.4). Visually hidden is fine.
    expect(live('steamgriddb').className).not.toMatch(/(^|\s)empty:hidden/);
  });

  it("a save is announced in the card's live region", async () => {
    await setup('steamgriddb', { enable: ['steamgriddb'] });
    await openCard('steamgriddb');
    await userEvent.type(screen.getByLabelText('Clave de API'), 'k-1');
    await userEvent.click(
      screen.getByRole('button', { name: 'Guardar Clave de API' }),
    );
    await waitFor(() =>
      expect(live('steamgriddb')).toHaveTextContent(
        'Clave de API: se ha guardado en el llavero.',
      ),
    );
  });

  it('a source turned off without a key cannot be turned back on', async () => {
    // The key was forgotten while the source was on (spec §9): the switch
    // follows the live setting, not the view it was first given.
    await setup('steamgriddb', {
      enable: ['steamgriddb', 'libretro'],
      over: { enabled: true },
    });
    await openCard('steamgriddb');
    const toggle = screen.getByRole('switch', { name: 'Activado' });
    expect(toggle).toBeChecked();
    expect(toggle).toBeEnabled();
    await userEvent.click(toggle);
    await waitFor(() => expect(toggle).not.toBeChecked());
    expect(toggle).toBeDisabled();
  });

  it('a failed forget says why and gives focus back to «Olvidar credenciales»', async () => {
    await setup('steamgriddb', { enable: ['steamgriddb'] });
    await openCard('steamgriddb');
    await userEvent.type(screen.getByLabelText('Clave de API'), 'k-1');
    await userEvent.click(
      screen.getByRole('button', { name: 'Guardar Clave de API' }),
    );
    vi.spyOn(backend, 'forgetIntegration').mockRejectedValueOnce({
      code: 'keychain-locked',
    });
    await userEvent.click(
      await screen.findByRole('button', { name: 'Olvidar credenciales' }),
    );
    await userEvent.click(screen.getByRole('button', { name: 'Olvidar' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(
      /El llavero de macOS está bloqueado/,
    );
    expect(
      screen.getByRole('button', { name: 'Olvidar credenciales' }),
    ).toHaveFocus();
  });

  it('help opens the official page by id, never a URL from the webview', async () => {
    await setup('steamgriddb');
    await openCard('steamgriddb');
    const help = vi.spyOn(backend, 'openIntegrationHelp');
    const url = vi.spyOn(backend, 'openLink');
    await userEvent.click(
      screen.getByRole('button', { name: /Cómo conseguir la clave/ }),
    );
    expect(help).toHaveBeenCalledWith('steamgriddb');
    expect(url).not.toHaveBeenCalled();
    expect(
      screen.getByText(
        'Inicia sesión con Steam, abre Preferencias › API y genera una clave.',
      ),
    ).toBeInTheDocument();
  });

  it('says what is sent and links its credit', async () => {
    await setup('screenscraper');
    await openCard('screenscraper');
    expect(screen.getByText('Qué se envía')).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: /Cómo conseguir la cuenta/ }),
    ).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole('button', { name: 'Crédito en Agradecimientos' }),
    );
    const nav = useNavStore.getState();
    expect(nav.section).toBe('credits');
    expect(nav.focusSetting).toBe('credit-screenscraper');
  });
});
