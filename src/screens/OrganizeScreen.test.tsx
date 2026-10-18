import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend } from '@/ipc/backend';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import { createMockBackend, type MockBackend } from '@/ipc/mock/mockBackend';
import { MOCK_DOWNLOADS, MOCK_MIXED } from '@/ipc/mock/mockOrganize';
import type { OrganizePlanView, OrganizeRequest } from '@/ipc/types';
import { resetLibraryStore, useLibraryStore } from '@/stores/libraryStore';
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

/** Confirms the undo the result screen asks about. */
async function confirmUndo() {
  const confirm = await screen.findByRole('alertdialog', {
    name: 'Undo this organisation?',
  });
  await userEvent.click(
    within(confirm).getByRole('button', { name: /^Undo( \d+ files)?$/ }),
  );
}

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
    const gba = await screen.findByRole('group', {
      name: /^ROMS\/GBA\/ \(6\)/,
    });
    expect(
      within(gba).getByRole('button', { name: /^ROMS\/GBA\/ \(6\)/ }),
    ).toHaveAttribute('aria-expanded', 'true');
    expect(
      within(gba).getByText(
        'Tetris Worlds (USA).zip → ROMS/GBA/Tetris Worlds (USA).zip',
      ),
    ).toBeInTheDocument();
    expect(within(gba).getAllByText('High').length).toBeGreaterThan(0);
    expect(
      within(gba).getByText(
        'CRC 7B729804 in the Game Boy Advance catalogue: Tetris Worlds (USA)',
      ),
    ).toBeInTheDocument();
    for (const folder of ['ROMS/GB/ (6)', 'ROMS/GBC/ (2)', 'ROMS/SFC/ (2)'])
      expect(screen.getByText(folder)).toBeInTheDocument();
    // Something moves: no "nothing to file away" empty state.
    expect(screen.queryByText('Nothing to file away')).not.toBeInTheDocument();
  });

  it('lists what stays in place with its reason', async () => {
    renderScreen(MOCK_DOWNLOADS);
    const left = (
      await screen.findByRole('heading', { name: 'Staying where they are' })
    ).closest('section')!;
    expect(within(left).getByText('3-D Tetris (USA).zip')).toBeInTheDocument();
    expect(within(left).getByText('Not a ROM (web page)')).toBeInTheDocument();
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
      within(bucket).getByRole('button', { name: 'Apply to 1 file' }),
    );
    const row = (
      await screen.findByText(
        'Pokemon Pinball.zip → ROMS/GBC/Pokemon Pinball.zip',
      )
    ).closest('li')!;
    expect(within(row).getByText('Chosen by you')).toBeInTheDocument();
    // A manual choice is not a confidence level.
    expect(within(row).queryByTitle('Confidence')).not.toBeInTheDocument();
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
    // Nothing moves: an empty state says so instead of an empty list.
    expect(screen.getByText('Nothing to file away')).toBeInTheDocument();
    expect(
      screen.getByText('No file here needs a system folder.'),
    ).toBeInTheDocument();
    await userEvent.click(button);
    await waitFor(() =>
      expect(open).toHaveBeenCalledWith(MOCK_DOWNLOADS, 'anbernic-stock'),
    );
  });

  it('does not offer to open another target for folders sorted in the source', async () => {
    tweakPlans(b, ({ plan }) => {
      plan.items = plan.items.map((i) => ({
        ...i,
        action: 'left' as const,
        reason: { kind: 'already-sorted' as const, folder: 'GBA' },
      }));
      plan.counts = { ...plan.counts, roms: 0, auto: 0, systems: 0 };
      plan.sorted_folders = [['ROMS/GBA', 'gba']];
      plan.target_root = '/Volumes/EMPTY';
      plan.roms_dir = '/Volumes/EMPTY/ROMS';
    });
    renderScreen(MOCK_DOWNLOADS);
    await ready();
    expect(
      screen.queryByRole('button', { name: 'Open library' }),
    ).not.toBeInTheDocument();
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

  it('shows progress while applying, with a cancel button', async () => {
    renderScreen(MOCK_DOWNLOADS);
    await ready();
    const planId = useOrganizeStore.getState().planId!;
    act(() =>
      useOrganizeStore.setState({
        status: 'applying',
        progress: { plan_id: planId, phase: 'apply', done: 4, total: 16 },
      }),
    );
    expect(screen.getByRole('status')).toHaveTextContent('Moving 4 of 16…');
    const bar = screen.getByRole('progressbar', {
      name: 'Organising progress',
    });
    expect(bar).toHaveAttribute('aria-valuenow', '4');
    expect(bar).toHaveAttribute('aria-valuemax', '16');
    expect(screen.getByRole('button', { name: 'Cancel' })).toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: /^Organise \d+ files$/ }),
    ).not.toBeInTheDocument();
  });

  it('back waits while files are being moved', async () => {
    renderScreen(MOCK_DOWNLOADS);
    await ready();
    const planId = useOrganizeStore.getState().planId!;
    act(() =>
      useOrganizeStore.setState({
        status: 'applying',
        progress: { plan_id: planId, phase: 'apply', done: 4, total: 16 },
      }),
    );
    expect(screen.getByRole('button', { name: 'Back' })).toBeDisabled();
  });

  it('says copying while a copy runs', async () => {
    renderScreen(MOCK_DOWNLOADS);
    await ready();
    const s = useOrganizeStore.getState();
    act(() =>
      useOrganizeStore.setState({
        status: 'applying',
        plan: { ...s.plan!, mode: 'copy' },
        progress: { plan_id: s.planId!, phase: 'apply', done: 8, total: 16 },
      }),
    );
    expect(screen.getByRole('status')).toHaveTextContent('Copying 8 of 16…');
  });

  it('shows the result counts when done, and the outcome of each file on demand', async () => {
    renderScreen(MOCK_DOWNLOADS);
    await userEvent.click(
      await screen.findByRole('button', { name: 'Organise 16 files' }),
    );
    expect(
      await screen.findByText('Moved: 16 · Copied: 0 · Skipped: 0 · Errors: 0'),
    ).toBeInTheDocument();
    expect(
      screen.queryByText('This is a proposal. Nothing has moved yet.'),
    ).not.toBeInTheDocument();
    const details = screen.getByRole('button', {
      name: 'What happened to each file',
    });
    expect(details).toHaveAttribute('aria-expanded', 'false');
    await userEvent.click(details);
    expect(
      screen.getAllByText(/^Tetris Worlds \(USA\)\.zip · Moved$/),
    ).toHaveLength(1);
  });

  it('open library opens the organised folder with the chosen layout', async () => {
    const open = vi.spyOn(b, 'openLibrary');
    renderScreen(MOCK_DOWNLOADS);
    await userEvent.click(
      await screen.findByRole('button', { name: 'Organise 16 files' }),
    );
    await userEvent.click(
      await screen.findByRole('button', { name: 'Open library' }),
    );
    await waitFor(() =>
      expect(open).toHaveBeenCalledWith(MOCK_DOWNLOADS, 'anbernic-stock'),
    );
    expect(useNavStore.getState().screen).toBe('main');
    expect(useLibraryStore.getState().view?.entries).toHaveLength(16);
  });

  it('undo restores and reports', async () => {
    renderScreen(MOCK_DOWNLOADS);
    await userEvent.click(
      await screen.findByRole('button', { name: 'Organise 16 files' }),
    );
    await userEvent.click(
      await screen.findByRole('button', { name: 'Undo 16 files' }),
    );
    await confirmUndo();
    expect(
      await screen.findByText(
        'Restored: 16 · Left alone because they changed: 0',
      ),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: /^Undo( \d+ files)?$/ }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: 'Open library' }),
    ).not.toBeInTheDocument();
    expect((await b.probeFolder(MOCK_DOWNLOADS)).rom_like).toBe(17);
  });

  it('undo lists later jobs and offers undo all in order', async () => {
    renderScreen(MOCK_DOWNLOADS);
    await userEvent.click(
      await screen.findByRole('button', { name: 'Organise 16 files' }),
    );
    await screen.findByRole('button', { name: 'Undo 16 files' });
    const lib = await b.openLibrary(MOCK_DOWNLOADS, 'anbernic-stock');
    await b.setEntryArtFromFile(lib.generation, 0, 'cover');
    await userEvent.click(
      screen.getByRole('button', { name: 'Undo 16 files' }),
    );
    await confirmUndo();
    expect(
      await screen.findByText(/^These must be undone first: Covers .+$/),
    ).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole('button', { name: 'Undo all in order' }),
    );
    expect(
      await screen.findByText(
        'Restored: 16 · Left alone because they changed: 0',
      ),
    ).toBeInTheDocument();
    expect(
      screen.queryByText(/^These must be undone first/),
    ).not.toBeInTheDocument();
  });

  it('a paused apply offers resume', async () => {
    b = createMockBackend({
      delayMs: 0,
      now: FIXTURE_NOW,
      storage: null,
      pauseAfter: null,
      organizePauseAfter: 8,
    });
    setBackend(b);
    renderScreen(MOCK_DOWNLOADS);
    await userEvent.click(
      await screen.findByRole('button', { name: 'Organise 16 files' }),
    );
    expect(
      await screen.findByText(
        'Paused: the SD card is full, read-only or was removed. Fix it and resume.',
      ),
    ).toBeInTheDocument();
    expect(screen.getByText('Disk full')).toBeInTheDocument();
    expect(
      screen.getByText('Moved: 8 · Copied: 0 · Skipped: 0 · Errors: 0'),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Resume' }));
    expect(
      await screen.findByText('Moved: 8 · Copied: 0 · Skipped: 0 · Errors: 0'),
    ).toBeInTheDocument();
    await waitFor(() =>
      expect(useOrganizeStore.getState().status).toBe('done'),
    );
    expect(
      screen.queryByRole('button', { name: 'Resume' }),
    ).not.toBeInTheDocument();
  });

  it('from a library: the layout is locked and back reopens the library', async () => {
    const open = vi.spyOn(b, 'openLibrary');
    useNavStore.getState().openOrganize(MOCK_DOWNLOADS, {
      layout: 'anbernic-stock',
      mode: 'move',
      locked: true,
    });
    render(
      <I18nProvider locale="en">
        <OrganizeScreen />
      </I18nProvider>,
    );
    await ready();
    expect(useOrganizeStore.getState().request).toMatchObject({
      source: MOCK_DOWNLOADS,
      layout: 'anbernic-stock',
      mode: 'move',
    });
    expect(screen.getByRole('combobox', { name: 'Layout' })).toBeDisabled();
    expect(screen.getByRole('combobox', { name: 'Target' })).toBeDisabled();
    await userEvent.click(
      screen.getByRole('button', { name: 'Organise 16 files' }),
    );
    await screen.findByText('Moved: 16 · Copied: 0 · Skipped: 0 · Errors: 0');
    await userEvent.click(screen.getByRole('button', { name: 'Back' }));
    await waitFor(() =>
      expect(open).toHaveBeenCalledWith(MOCK_DOWNLOADS, 'anbernic-stock'),
    );
    expect(useNavStore.getState().screen).toBe('main');
  });
});
