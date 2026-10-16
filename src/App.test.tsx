import { clearMocks, mockIPC } from '@tauri-apps/api/mocks';
import { render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';

import App from './App';
import { I18nProvider } from './i18n/I18nProvider';

function renderApp(locale: 'en' | 'es' = 'en') {
  return render(
    <I18nProvider locale={locale}>
      <App />
    </I18nProvider>,
  );
}

describe('App start screen', () => {
  afterEach(() => {
    clearMocks();
  });

  it('shows the wordmark and the tagline', () => {
    renderApp();
    expect(
      screen.getByRole('heading', { level: 1, name: 'Coverly ROMs' }),
    ).toBeInTheDocument();
    expect(
      screen.getByText('Find and add the missing cover art in your ROMs'),
    ).toBeInTheDocument();
  });

  it('shows the tagline in Spanish', () => {
    renderApp('es');
    expect(
      screen.getByText('Encuentra y añade las portadas que faltan en tus ROMs'),
    ).toBeInTheDocument();
  });

  it('has a disabled Open library button', () => {
    renderApp();
    expect(
      screen.getByRole('button', { name: 'Open library…' }),
    ).toBeDisabled();
  });

  it('renders a dash for versions outside Tauri', () => {
    renderApp();
    expect(screen.getByText('Version —')).toBeInTheDocument();
    expect(screen.getByText('Core —')).toBeInTheDocument();
  });

  it('shows the versions reported by app_info inside Tauri', async () => {
    mockIPC((cmd) => {
      if (cmd === 'app_info') {
        return {
          name: 'Coverly ROMs',
          version: '9.9.9',
          core_version: '1.2.3',
        };
      }
      return undefined;
    });
    renderApp();
    expect(await screen.findByText('Version 9.9.9')).toBeInTheDocument();
    expect(screen.getByText('Core 1.2.3')).toBeInTheDocument();
  });
});
