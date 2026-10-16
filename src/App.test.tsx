import { clearMocks, mockIPC } from '@tauri-apps/api/mocks';
import { render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import App from './App';
import { I18nProvider } from './i18n/I18nProvider';
import { setBackend } from './ipc/backend';
import { createMockBackend } from './ipc/mock/mockBackend';
import { createTauriBackend } from './ipc/tauriBackend';
import { resetLibraryStore } from './stores/libraryStore';

function renderApp(locale: 'en' | 'es' = 'en') {
  return render(
    <I18nProvider locale={locale}>
      <App />
    </I18nProvider>,
  );
}

describe('App start screen', () => {
  beforeEach(() => {
    setBackend(createMockBackend({ delayMs: 0 }));
    resetLibraryStore();
  });
  afterEach(() => {
    setBackend(null);
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

  it('has an enabled Open folder button', () => {
    renderApp();
    expect(screen.getByRole('button', { name: 'Open folder…' })).toBeEnabled();
  });

  it('renders a dash for versions until the backend answers', () => {
    renderApp();
    expect(screen.getByText('Version —')).toBeInTheDocument();
    expect(screen.getByText('Core —')).toBeInTheDocument();
  });

  it('shows the versions reported by app_info inside Tauri', async () => {
    mockIPC((cmd) =>
      cmd === 'app_info'
        ? { name: 'Coverly ROMs', version: '9.9.9', core_version: '1.2.3' }
        : undefined,
    );
    setBackend(createTauriBackend());
    renderApp();
    expect(await screen.findByText('Version 9.9.9')).toBeInTheDocument();
    expect(screen.getByText('Core 1.2.3')).toBeInTheDocument();
  });
});
