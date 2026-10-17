import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend } from '@/ipc/backend';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import { createMockBackend, type MockBackend } from '@/ipc/mock/mockBackend';
import { MOCK_DOWNLOADS, MOCK_MIXED } from '@/ipc/mock/mockOrganize';
import type { OrganizePlanView, OrganizeRequest } from '@/ipc/types';
import { resetLibraryStore } from '@/stores/libraryStore';
import { useNavStore } from '@/stores/navStore';
import { useOrganizeStore } from '@/stores/organizeStore';

import { OrganizeScreen } from './OrganizeScreen';

function mock(): MockBackend {
  return createMockBackend({
    delayMs: 0,
    now: FIXTURE_NOW,
    storage: null,
    pauseAfter: null,
  });
}

/** Rewrites every plan the mock answers (a read-only source, a detected target…). */
function tweakPlans(
  b: MockBackend,
  f: (view: OrganizePlanView, request: OrganizeRequest) => void,
) {
  const real = b.organizePlan.bind(b);
  vi.spyOn(b, 'organizePlan').mockImplementation(async (req, d) => {
    const view = await real(req, d);
    f(view, req);
    return view;
  });
}

function renderScreen(source: string) {
  useNavStore.getState().openOrganize(source);
  return render(
    <I18nProvider locale="en">
      <OrganizeScreen />
    </I18nProvider>,
  );
}

const ready = () =>
  waitFor(() => expect(useOrganizeStore.getState().status).toBe('ready'));

describe('OrganizeScreen', () => {
  let b: MockBackend;
  beforeEach(() => {
    b = mock();
    setBackend(b);
    resetLibraryStore();
    useOrganizeStore.getState().reset();
  });
  afterEach(() => {
    useOrganizeStore.getState().reset();
    useNavStore.getState().closeOrganize();
    vi.restoreAllMocks();
    setBackend(null);
  });

  it('plans the folder it was opened for and shows the summary and the dry-run hint', async () => {
    renderScreen(MOCK_DOWNLOADS);
    expect(
      await screen.findByText(
        'ROMs: 16 · Systems: 4 · To review: 0 · Staying put: 4',
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByText('This is a proposal. Nothing has moved yet.'),
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Organise 16 files' }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole('heading', { name: 'To review' }),
    ).not.toBeInTheDocument();
  });

  it('groups by system with target folder, confidence and why', async () => {
    renderScreen(MOCK_DOWNLOADS);
    const gba = (await screen.findByText('ROMS/GBA/ (6)')).closest('details')!;
    expect(gba).toHaveAttribute('open');
    expect(
      within(gba).getByText(
        'Tetris Worlds (USA).zip → ROMS/GBA/Tetris Worlds (USA).zip',
      ),
    ).toBeInTheDocument();
    expect(within(gba).getAllByText('High').length).toBeGreaterThan(0);
    expect(
      within(gba).getByText(
        'CRC 1A2B3C4D in the Game Boy Advance catalogue: Tetris Worlds (USA)',
      ),
    ).toBeInTheDocument();
    for (const folder of ['ROMS/GB/ (5)', 'ROMS/GBC/ (2)', 'ROMS/SFC/ (3)'])
      expect(screen.getByText(folder)).toBeInTheDocument();
  });

  it('lists what stays in place with its reason', async () => {
    renderScreen(MOCK_DOWNLOADS);
    const left = (
      await screen.findByRole('heading', { name: 'Staying where they are' })
    ).closest('section')!;
    expect(within(left).getByText('Tetris Attack (1).zip')).toBeInTheDocument();
    expect(
      within(left).getByText('Same as Tetris Attack.zip'),
    ).toBeInTheDocument();
    expect(within(left).getAllByText('Download not finished')).toHaveLength(3);
  });

  it('settles a review bucket with apply to all', async () => {
    renderScreen(MOCK_MIXED);
    const bucket = await screen.findByRole('group', {
      name: 'Game Boy / Game Boy Color: 1',
    });
    await userEvent.selectOptions(
      within(bucket).getByRole('combobox', {
        name: 'Game Boy / Game Boy Color: 1',
      }),
      'gbc',
    );
    await userEvent.click(
      within(bucket).getByRole('button', { name: 'Apply to all' }),
    );
    expect(
      await screen.findByText(
        'Pokemon Pinball.zip → ROMS/GBC/Pokemon Pinball.zip',
      ),
    ).toBeInTheDocument();
    expect(screen.getByText('Chosen by you')).toBeInTheDocument();
    expect(
      screen.queryByRole('group', { name: 'Game Boy / Game Boy Color: 1' }),
    ).not.toBeInTheDocument();
  });

  it('lets the user rescue an unknown file with the system select', async () => {
    renderScreen(MOCK_MIXED);
    const select = await screen.findByRole('combobox', {
      name: 'System for mystery.rom',
    });
    const names = within(select)
      .getAllByRole('option')
      .map((o) => o.textContent);
    // A prompt, then every system by name.
    expect(names[0]).toBe('Choose a system…');
    expect(names.slice(1)).toEqual(
      [...names.slice(1)].sort((x, y) =>
        x!.localeCompare(y!, 'en', { sensitivity: 'base' }),
      ),
    );
    await userEvent.selectOptions(select, 'gb');
    expect(
      await screen.findByText('mystery.rom → ROMS/GB/mystery.rom'),
    ).toBeInTheDocument();
  });

  it('changing the layout re-plans and shows esde folder names', async () => {
    renderScreen(MOCK_DOWNLOADS);
    await screen.findByText('ROMS/GBA/ (6)');
    const layout = screen.getByRole('combobox', { name: 'Layout' });
    await waitFor(() =>
      expect(
        within(layout).getByRole('option', { name: 'ES-DE' }),
      ).toBeInTheDocument(),
    );
    await userEvent.selectOptions(layout, 'esde');
    expect(await screen.findByText('ROMs/gba/ (6)')).toBeInTheDocument();
    expect(useOrganizeStore.getState().request).toMatchObject({
      layout: 'esde',
    });
  });

  it('nests in the layout ROMs folder or puts the folders directly here', async () => {
    renderScreen(MOCK_DOWNLOADS);
    await screen.findByText('ROMS/GBA/ (6)');
    const folders = screen.getByRole('combobox', { name: 'Folders' });
    expect(
      within(folders)
        .getAllByRole('option')
        .map((o) => o.textContent),
    ).toEqual(['Inside ROMS/', 'Directly here']);
    await userEvent.selectOptions(folders, 'flat');
    expect(await screen.findByText('GBA/ (6)')).toBeInTheDocument();
    // RetroArch has no ROMs folder: the choice disappears.
    await userEvent.selectOptions(
      screen.getByRole('combobox', { name: 'Layout' }),
      'retroarch',
    );
    await ready();
    await waitFor(() =>
      expect(
        screen.queryByRole('combobox', { name: 'Folders' }),
      ).not.toBeInTheDocument(),
    );
  });

  it('move is disabled with the reason when the source is read-only', async () => {
    tweakPlans(b, ({ plan }) => {
      plan.move_allowed = false;
      plan.mode = 'copy';
      plan.warnings.push({ kind: 'move-unavailable' });
    });
    renderScreen(MOCK_DOWNLOADS);
    const mode = await screen.findByRole('combobox', { name: 'Mode' });
    await ready();
    expect(mode).toHaveValue('copy');
    const move = within(mode).getByRole('option', { name: 'Move' });
    expect(move).toBeDisabled();
    expect(move).toHaveAttribute(
      'title',
      "Can't move: the source folder is read-only.",
    );
    expect(
      screen.getByText("Can't move: the source folder is read-only."),
    ).toBeInTheDocument();
  });

  it('switches to the layout of a target that is already a library, and warns on a mismatch', async () => {
    vi.spyOn(b, 'pickFolder').mockResolvedValue('/Volumes/SD');
    tweakPlans(b, ({ plan }, req) => {
      if (req.target !== '/Volumes/SD' || plan.layout === 'esde') return;
      plan.target_detected = 'esde';
      plan.warnings.push({ kind: 'target-layout-mismatch', profile: 'esde' });
    });
    renderScreen(MOCK_DOWNLOADS);
    await screen.findByText('ROMS/GBA/ (6)');
    await userEvent.selectOptions(
      screen.getByRole('combobox', { name: 'Target' }),
      'other',
    );
    await waitFor(() =>
      expect(useOrganizeStore.getState().request).toMatchObject({
        target: '/Volumes/SD',
        layout: 'esde',
      }),
    );
    await ready();
    expect(screen.getByRole('combobox', { name: 'Target' })).toHaveValue(
      '/Volumes/SD',
    );
    expect(
      await screen.findByText(
        'The target is already a ES-DE library; that layout is used.',
      ),
    ).toBeInTheDocument();
    // Picking another layout on purpose is allowed, with a warning.
    await userEvent.selectOptions(
      screen.getByRole('combobox', { name: 'Layout' }),
      'muos',
    );
    expect(
      await screen.findByText(
        "The target uses ES-DE; new folders won't show up on that system.",
      ),
    ).toBeInTheDocument();
    expect(useOrganizeStore.getState().request).toMatchObject({
      layout: 'muos',
    });
  });

  it('hides the primary action when nothing would move and offers open library', async () => {
    tweakPlans(b, ({ plan }) => {
      plan.items = plan.items.map((i) => ({
        ...i,
        action: 'left' as const,
        reason: { kind: 'already-sorted' as const, folder: 'GBA' },
      }));
      plan.counts = { ...plan.counts, roms: 0, auto: 0, systems: 0 };
      plan.sorted_folders = [['ROMS/GBA', 'gba']];
    });
    const open = vi.spyOn(b, 'openLibrary');
    renderScreen(MOCK_DOWNLOADS);
    const button = await screen.findByRole('button', { name: 'Open library' });
    expect(
      screen.queryByRole('button', { name: /^Organise \d+ files$/ }),
    ).not.toBeInTheDocument();
    await userEvent.click(button);
    await waitFor(() =>
      expect(open).toHaveBeenCalledWith(MOCK_DOWNLOADS, 'anbernic-stock'),
    );
  });

  it('renders hostile file names as text', async () => {
    const { container } = renderScreen(MOCK_MIXED);
    expect(
      await screen.findByText(
        '<img src=x onerror=alert(1)> Racer (USA).sfc → ROMS/SFC/<img src=x onerror=alert(1)> Racer (USA).sfc',
      ),
    ).toBeInTheDocument();
    expect(container.querySelector('img')).toBeNull();
  });

  it('shows planning progress with a cancel button', async () => {
    renderScreen(MOCK_DOWNLOADS);
    const status = screen.getByRole('status');
    expect(status).toHaveTextContent(
      /^(Looking for files…|Identifying \d+ of \d+…)$/,
    );
    expect(screen.getByRole('button', { name: 'Cancel' })).toBeInTheDocument();
    await ready();
    expect(screen.queryByRole('status')).not.toBeInTheDocument();
  });

  it('back resets the proposal and leaves the screen', async () => {
    renderScreen(MOCK_DOWNLOADS);
    await ready();
    await userEvent.click(screen.getByRole('button', { name: 'Back' }));
    expect(useNavStore.getState().screen).toBe('main');
    expect(useOrganizeStore.getState().status).toBe('idle');
  });
});
