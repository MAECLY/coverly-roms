import { act, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { version } from '../../package.json';
import { Root } from '@/Root';
import { setBackend } from '@/ipc/backend';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import { createMockBackend } from '@/ipc/mock/mockBackend';
import { resetSettingsStore } from '@/stores/settingsStore';
import { resetNavStore, useNavStore } from '@/stores/navStore';
import { MemoryStorage } from '@/test/storage';

describe('versions', () => {
  beforeEach(() => {
    setBackend(
      createMockBackend({
        delayMs: 0,
        now: FIXTURE_NOW,
        storage: new MemoryStorage(),
      }),
    );
    resetSettingsStore();
    resetNavStore();
  });
  afterEach(() => setBackend(null));

  it('the footer shows the app and core versions instead of a dash', async () => {
    render(<Root />);
    expect(await screen.findByText(`Version ${version}`)).toBeInTheDocument();
    expect(screen.getByText(`Core ${version}`)).toBeInTheDocument();
    expect(screen.queryByText('Version —')).not.toBeInTheDocument();
  });

  it('About shows the same versions', async () => {
    render(<Root />);
    await screen.findByRole('button', { name: 'Settings' });
    act(() => useNavStore.getState().openSettings('about'));
    const panel = await screen.findByRole('tabpanel');
    expect(
      await within(panel).findByText(`Version ${version}`),
    ).toBeInTheDocument();
    expect(within(panel).getByText(`Core ${version}`)).toBeInTheDocument();
  });
});
