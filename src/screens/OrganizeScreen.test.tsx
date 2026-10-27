import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react';
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
        'ROMs: 19 · Systems: 7 · To review: 0 · Staying put: 4',
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByText('This is a proposal. Nothing has moved yet.'),
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Organise 19 files' }),
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

  it('warns when the target lacks room, with both sizes', async () => {
    tweakPlans(b, ({ plan }) => {
      plan.warnings.push({
        kind: 'no-space',
        // The three disc games: same decimal units as the extract summary.
        needed: 2_188_698_846,
        free: 1_500_000_000,
      });
    });
    renderScreen(MOCK_DOWNLOADS);
    expect(
      await screen.findByText(
        'Not enough space on the target: 2.19 GB needed, 1.5 GB free.',
      ),
    ).toBeInTheDocument();
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

  it('summary_shows_extract_count_and_bytes', async () => {
    renderScreen(MOCK_DOWNLOADS);
    await ready();
    const plan = useOrganizeStore.getState().plan!;
    const discs = plan.items.filter(
      (i) => i.action === 'auto' && i.transfer.kind === 'extract',
    );
    expect(discs).toHaveLength(3);
    const bytes = discs.reduce(
      (n, i) => n + (i.transfer.kind === 'extract' ? i.transfer.bytes : 0),
      0,
    );
    // Decimal units, like the CLI and the spec: 2,188,698,846 B.
    expect(bytes).toBe(2_188_698_846);
    expect(
      screen.getByText('Extracting: 3 games (2.19 GB)'),
    ).toBeInTheDocument();
    // The apply button still counts items, not extracted files.
    expect(
      screen.getByRole('button', { name: 'Organise 19 files' }),
    ).toBeInTheDocument();
  });

  it('disc_rows_show_spec_sizes_and_system_names', async () => {
    renderScreen(MOCK_DOWNLOADS);
    await ready();
    expect(screen.getByText('12 files · 405.7 MB')).toBeInTheDocument();
    expect(screen.getByText('16 files · 1.19 GB')).toBeInTheDocument();
    expect(screen.getByText('52 files · 591.9 MB')).toBeInTheDocument();
    // Saturn is named like the Rust catalogue, never by its id.
    expect(
      screen.getByRole('button', { name: 'ROMS/SATURN/ (1)Sega Saturn' }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        'Sega Saturn disc: IP.BIN SEGA SEGASATURN T-5704H (T-5704H)',
      ),
    ).toBeInTheDocument();
  });

  it('no extract summary when nothing is extracted', async () => {
    renderScreen(MOCK_MIXED);
    await ready();
    expect(screen.queryByText(/^Extracting: /)).not.toBeInTheDocument();
  });

  it('progress_shows_extracting_name_and_counts', async () => {
    renderScreen(MOCK_DOWNLOADS);
    await ready();
    const s = useOrganizeStore.getState();
    const disc = s.plan!.items.find(
      (i) => i.sources[0] === 'Next Tetris, The.zip',
    )!;
    act(() =>
      useOrganizeStore.setState({
        status: 'applying',
        progress: {
          plan_id: s.planId!,
          phase: 'extract',
          item: disc.id,
          done: 5,
          total: 19,
          bytes_done: 300,
          bytes_total: 1200,
        },
      }),
    );
    expect(screen.getByRole('status')).toHaveTextContent(
      'Extracting Next Tetris, The.zip: 5 of 19 files…',
    );
    // The bar follows the bytes of the game being extracted.
    const bar = screen.getByRole('progressbar', {
      name: 'Organising progress',
    });
    expect(bar).toHaveAttribute('aria-valuenow', '300');
    expect(bar).toHaveAttribute('aria-valuemax', '1200');
  });

  it('follows extract progress events while applying', async () => {
    renderScreen(MOCK_DOWNLOADS);
    await userEvent.click(
      await screen.findByRole('button', { name: 'Organise 19 files' }),
    );
    expect(
      await screen.findByText(/^Extracting .+\.zip: \d+ of 19 files…$/),
    ).toBeInTheDocument();
    expect(
      await screen.findByText(
        'Moved: 16 · Copied: 0 · Skipped: 0 · Errors: 0 · Extracted: 3',
      ),
    ).toBeInTheDocument();
  });

  it('a RAR without its tool says what to install and scans again', async () => {
    b.organizeTouch(MOCK_DOWNLOADS, { add: ['Tetris Collection.rar'] });
    renderScreen(MOCK_DOWNLOADS);
    const left = (
      await screen.findByRole('heading', { name: 'Staying where they are' })
    ).closest('section')!;
    const row = within(left).getByText('Tetris Collection.rar').closest('li')!;
    expect(
      within(row).getByText('To open RAR archives, install unrar'),
    ).toBeInTheDocument();
    const plans = vi.spyOn(b, 'organizePlan');
    await userEvent.click(
      within(row).getByRole('button', { name: 'Scan again' }),
    );
    await waitFor(() => expect(plans).toHaveBeenCalled());
    expect(plans.mock.calls[0]![0]).toMatchObject({ rescan: true });
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
      await screen.findByRole('button', { name: 'Organise 19 files' }),
    );
    expect(
      await screen.findByText(
        'Moved: 16 · Copied: 0 · Skipped: 0 · Errors: 0 · Extracted: 3',
      ),
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
      await screen.findByRole('button', { name: 'Organise 19 files' }),
    );
    await userEvent.click(
      await screen.findByRole('button', { name: 'Open library' }),
    );
    await waitFor(() =>
      expect(open).toHaveBeenCalledWith(MOCK_DOWNLOADS, 'anbernic-stock'),
    );
    expect(useNavStore.getState().screen).toBe('main');
    expect(useLibraryStore.getState().view?.entries).toHaveLength(20);
  });

  it('undo restores and reports', async () => {
    renderScreen(MOCK_DOWNLOADS);
    await userEvent.click(
      await screen.findByRole('button', { name: 'Organise 19 files' }),
    );
    await userEvent.click(
      await screen.findByRole('button', { name: 'Undo 19 files' }),
    );
    await confirmUndo();
    expect(
      await screen.findByText(
        'Restored: 98 · Left alone because they changed: 0',
      ),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: /^Undo( \d+ files)?$/ }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: 'Open library' }),
    ).not.toBeInTheDocument();
    expect((await b.probeFolder(MOCK_DOWNLOADS)).rom_like).toBe(20);
  });

  it('undo lists later jobs and offers undo all in order', async () => {
    renderScreen(MOCK_DOWNLOADS);
    await userEvent.click(
      await screen.findByRole('button', { name: 'Organise 19 files' }),
    );
    await screen.findByRole('button', { name: 'Undo 19 files' });
    const lib = await b.openLibrary(MOCK_DOWNLOADS, 'anbernic-stock');
    await b.setEntryArtFromFile(lib.generation, 0, 'cover');
    await userEvent.click(
      screen.getByRole('button', { name: 'Undo 19 files' }),
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
        'Restored: 98 · Left alone because they changed: 0',
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
      await screen.findByRole('button', { name: 'Organise 19 files' }),
    );
    expect(
      await screen.findByText(
        'Paused: the SD card is full, read-only or was removed. Fix it and resume.',
      ),
    ).toBeInTheDocument();
    expect(screen.getByText('Disk full')).toBeInTheDocument();
    // The first 8 items: 6 cartridges moved and 2 discs extracted.
    expect(
      screen.getByText(
        'Moved: 6 · Copied: 0 · Skipped: 0 · Errors: 0 · Extracted: 2',
      ),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Resume' }));
    expect(
      await screen.findByText(
        'Moved: 10 · Copied: 0 · Skipped: 0 · Errors: 0 · Extracted: 1',
      ),
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
      screen.getByRole('button', { name: 'Organise 19 files' }),
    );
    await screen.findByText(
      'Moved: 16 · Copied: 0 · Skipped: 0 · Errors: 0 · Extracted: 3',
    );
    await userEvent.click(screen.getByRole('button', { name: 'Back' }));
    await waitFor(() =>
      expect(open).toHaveBeenCalledWith(MOCK_DOWNLOADS, 'anbernic-stock'),
    );
    expect(useNavStore.getState().screen).toBe('main');
  });
  describe('scan again', () => {
    const scanAgain = () => screen.getByRole('button', { name: 'Scan again' });

    it('reads the folder again, says what changed and marks the new files', async () => {
      renderScreen(MOCK_MIXED);
      await ready();
      b.organizeTouch(MOCK_MIXED, {
        add: ['Wario Land 4 (USA).gba'],
        remove: ['mystery.rom'],
      });
      const plan = vi.spyOn(b, 'organizePlan');
      await userEvent.click(scanAgain());
      expect(plan.mock.calls[0]![0]).toMatchObject({ rescan: true });
      const summary = await screen.findByText(
        'The folder changed. New: 1 · Gone: 1',
      );
      expect(summary.closest('[role="status"]')).not.toBeNull();
      const row = screen
        .getByText(/^Wario Land 4 \(USA\)\.gba →/)
        .closest('li')!;
      expect(within(row).getByText('New')).toBeInTheDocument();
      expect(
        screen.getByRole('button', { name: /GBA\/ \(1\).*New: 1/ }),
      ).toBeInTheDocument();
      expect(screen.queryByText('mystery.rom')).not.toBeInTheDocument();
      // Dismissed, the summary and the marks stay until the next change.
      await userEvent.click(screen.getByRole('button', { name: 'Dismiss' }));
      expect(
        screen.queryByText('The folder changed. New: 1 · Gone: 1'),
      ).not.toBeInTheDocument();
    });

    it('answers even when nothing changed, and keeps the choices made', async () => {
      renderScreen(MOCK_MIXED);
      await ready();
      await userEvent.selectOptions(
        screen.getByRole('combobox', {
          name: 'System for mystery.rom',
        }),
        'gba',
      );
      await ready();
      await userEvent.click(scanAgain());
      expect(
        await screen.findByText('No changes. The folder holds the same files.'),
      ).toBeInTheDocument();
      expect(screen.getByText('Your choices kept: 1')).toBeInTheDocument();
      expect(screen.queryByText('New')).not.toBeInTheDocument();
    });

    it('is disabled while a plan is made and busy while it scans', async () => {
      renderScreen(MOCK_MIXED);
      expect(scanAgain()).toBeDisabled();
      await ready();
      expect(scanAgain()).toBeEnabled();
      act(() => void useOrganizeStore.getState().refresh());
      expect(scanAgain()).toHaveAttribute('aria-busy', 'true');
      // Its own scan: inert but still focusable, so focus is not lost.
      expect(scanAgain()).toHaveAttribute('aria-disabled', 'true');
      await ready();
      expect(scanAgain()).not.toHaveAttribute('aria-busy');
      expect(scanAgain()).not.toHaveAttribute('aria-disabled');
    });

    it('keeps keyboard focus on the button through the scan', async () => {
      renderScreen(MOCK_MIXED);
      await ready();
      b.organizeTouch(MOCK_MIXED, { add: 1 });
      scanAgain().focus();
      await userEvent.keyboard('{Enter}');
      expect(scanAgain()).toHaveFocus();
      // A second press while it scans starts nothing.
      const plan = vi.spyOn(b, 'organizePlan');
      await userEvent.keyboard('{Enter}');
      expect(plan).not.toHaveBeenCalled();
      await screen.findByText('The folder changed. New: 1 · Gone: 0');
      expect(scanAgain()).toHaveFocus();
    });

    it('try again hands focus back to the scan button', async () => {
      renderScreen(MOCK_MIXED);
      await ready();
      b.organizeTouch(MOCK_MIXED, { gone: true });
      await userEvent.click(scanAgain());
      const alert = await screen.findByRole('alert');
      b.organizeTouch(MOCK_MIXED, { gone: false });
      await userEvent.click(
        within(alert).getByRole('button', { name: 'Try again' }),
      );
      await screen.findByText('No changes. The folder holds the same files.');
      expect(scanAgain()).toHaveFocus();
    });

    it('⌘R or Ctrl+R scans again instead of reloading the page', async () => {
      renderScreen(MOCK_MIXED);
      await ready();
      const plan = vi.spyOn(b, 'organizePlan');
      expect(fireEvent.keyDown(window, { key: 'r', metaKey: true })).toBe(
        false,
      );
      await ready();
      expect(plan).toHaveBeenCalledTimes(1);
      expect(plan.mock.calls[0]![0]).toMatchObject({ rescan: true });
      expect(fireEvent.keyDown(window, { key: 'R', ctrlKey: true })).toBe(
        false,
      );
      await ready();
      expect(plan).toHaveBeenCalledTimes(2);
      // Typing in a text field: no scan, and still no reload.
      const input = document.createElement('input');
      document.body.append(input);
      expect(fireEvent.keyDown(input, { key: 'r', metaKey: true })).toBe(false);
      input.remove();
      expect(plan).toHaveBeenCalledTimes(2);
      // A plain R is just a key.
      expect(fireEvent.keyDown(window, { key: 'r' })).toBe(true);
    });

    it('⌘R works right after picking a system in a select', async () => {
      renderScreen(MOCK_MIXED);
      await ready();
      const select = screen.getByRole('combobox', {
        name: 'System for mystery.rom',
      });
      await userEvent.selectOptions(select, 'gba');
      await ready();
      const plan = vi.spyOn(b, 'organizePlan');
      const picked = screen.getByRole('combobox', {
        name: 'System for mystery.rom',
      });
      expect(fireEvent.keyDown(picked, { key: 'r', metaKey: true })).toBe(
        false,
      );
      await ready();
      expect(plan.mock.calls[0]![0]).toMatchObject({ rescan: true });
    });

    it('a folder that went away keeps the plan, blocks apply and offers try again', async () => {
      renderScreen(MOCK_DOWNLOADS);
      await ready();
      b.organizeTouch(MOCK_DOWNLOADS, { gone: true });
      await userEvent.click(scanAgain());
      const alert = await screen.findByRole('alert');
      expect(alert).toHaveTextContent(
        `This folder does not exist: ${MOCK_DOWNLOADS}`,
      );
      expect(alert).toHaveTextContent('Nothing was moved.');
      expect(
        screen.getByRole('button', { name: 'Organise 19 files' }),
      ).toBeDisabled();
      expect(screen.getByRole('button', { name: 'Back' })).toBeEnabled();
      b.organizeTouch(MOCK_DOWNLOADS, { gone: false });
      await userEvent.click(
        within(alert).getByRole('button', { name: 'Try again' }),
      );
      await ready();
      expect(screen.queryByRole('alert')).not.toBeInTheDocument();
      expect(
        screen.getByRole('button', { name: 'Organise 19 files' }),
      ).toBeEnabled();
    });

    it('is not offered once the files were organised', async () => {
      renderScreen(MOCK_DOWNLOADS);
      await userEvent.click(
        await screen.findByRole('button', { name: 'Organise 19 files' }),
      );
      await screen.findByText(
        'Moved: 16 · Copied: 0 · Skipped: 0 · Errors: 0 · Extracted: 3',
      );
      expect(
        screen.queryByRole('button', { name: 'Scan again' }),
      ).not.toBeInTheDocument();
      const plan = vi.spyOn(b, 'organizePlan');
      fireEvent.keyDown(window, { key: 'r', metaKey: true });
      expect(plan).not.toHaveBeenCalled();
    });
  });

  describe('duplicates step', () => {
    it('organize_options_picker_defaults_to_set_aside', async () => {
      renderScreen(MOCK_MIXED);
      await ready();
      const picker = screen.getByRole('combobox', { name: 'Duplicates' });
      expect(picker).toHaveValue('set-aside');
      expect(
        within(picker)
          .getAllByRole('option')
          .map((o) => o.textContent),
      ).toEqual(['Set aside', 'Report only', "Don't look"]);
      expect(screen.getByText('Duplicates: 1')).toBeInTheDocument();
      const plan = vi.spyOn(b, 'organizePlan');
      await userEvent.selectOptions(picker, 'report');
      await ready();
      expect(plan.mock.calls.at(-1)![0]).toMatchObject({
        duplicates: 'report',
      });
      expect(screen.getByRole('combobox', { name: 'Duplicates' })).toHaveValue(
        'report',
      );
      expect(useOrganizeStore.getState().plan!.counts.set_aside).toBe(0);
      expect(
        screen.getByText(
          'Already in your library as ROMS/GB/Super Mario Land (World).gb',
        ),
      ).toBeInTheDocument();
      expect(
        screen.getByRole('button', { name: 'Organise 4 files' }),
      ).toBeInTheDocument();
      await userEvent.selectOptions(
        screen.getByRole('combobox', { name: 'Duplicates' }),
        'off',
      );
      await ready();
      expect(
        screen.queryByRole('heading', { name: 'Duplicates' }),
      ).not.toBeInTheDocument();
    });

    it('duplicate groups show no cover column in the proposal', async () => {
      const { container } = renderScreen(MOCK_MIXED);
      await ready();
      // A small proposal starts open.
      expect(container.querySelector('.dup-group')).not.toBeNull();
      expect(container.querySelector('.dup-cover')).toBeNull();
      expect(container.querySelector('.dup-own-art')).toBeNull();
    });

    it('duplicate groups show tags but no copy numbers in the proposal', async () => {
      const { container } = renderScreen(MOCK_MIXED);
      await ready();
      const group = screen.getByRole('group', {
        name: 'Super Mario Land (World) (Rev 1)',
      });
      // Numbers are pick keys; nothing is picked here.
      expect(container.querySelector('.dup-row__num')).toBeNull();
      // The catalogue name's region and revision, like Rust's group_units.
      expect(within(group).getAllByText('WLD').length).toBeGreaterThan(0);
      expect(within(group).getAllByText('Rev 1').length).toBeGreaterThan(0);
    });

    it('a pile copy reveals its own file', async () => {
      renderScreen(MOCK_MIXED);
      await ready();
      const reveal = vi.spyOn(b, 'revealPath');
      const group = screen.getByRole('group', {
        name: 'Super Mario Land (World) (Rev 1)',
      });
      const button = within(group).getByRole('button', {
        name: /^Show Super Mario Land \(World\) \(1\)\.gb in its folder$/,
      });
      await userEvent.click(button);
      expect(reveal).toHaveBeenCalledTimes(1);
      expect(reveal.mock.calls[0]![0]).toMatch(
        /Super Mario Land \(World\) \(1\)\.gb$/,
      );
      await waitFor(() => expect(useOrganizeStore.getState().error).toBeNull());
    });

    it('the_picker_starts_from_the_setting', async () => {
      await b.updateSettings({ dedupe: { organize_step: 'off' } });
      renderScreen(MOCK_MIXED);
      await ready();
      await waitFor(() =>
        expect(
          screen.getByRole('combobox', { name: 'Duplicates' }),
        ).toHaveValue('off'),
      );
    });

    it('organize_apply_label_counts_set_aside', async () => {
      renderScreen(MOCK_MIXED);
      const apply = await screen.findByRole('button', {
        name: 'Organise 4 files and set aside 1',
      });
      await userEvent.click(apply);
      expect(
        await screen.findByText(
          'Moved: 4 · Copied: 0 · Skipped: 2 · Errors: 0 · Set aside: 1',
        ),
      ).toBeInTheDocument();
    });

    it('the_trash_setting_names_the_trash_on_the_button_and_the_result', async () => {
      await b.updateSettings({ dedupe: { action: 'trash' } });
      renderScreen(MOCK_MIXED);
      await userEvent.click(
        await screen.findByRole('button', {
          name: 'Organise 4 files and send 1 to the Trash',
        }),
      );
      expect(
        await screen.findByText(
          'Moved: 4 · Copied: 0 · Skipped: 2 · Errors: 0 · In the Trash: 1',
        ),
      ).toBeInTheDocument();
      expect(
        screen.getByRole('button', { name: 'Undo 5 files' }),
      ).toBeInTheDocument();
    });

    it('organize_section_reuses_dup_group', async () => {
      const { container } = renderScreen(MOCK_MIXED);
      await ready();
      const section = screen
        .getByRole('heading', { name: 'Duplicates' })
        .closest('section')!;
      const groups = container.querySelectorAll('[data-dup-group]');
      expect(groups).toHaveLength(1);
      expect(section).toContainElement(groups[0] as HTMLElement);
      const group = within(section).getByRole('group', {
        name: 'Super Mario Land (World) (Rev 1)',
      });
      expect(
        within(group).getByText('Super Mario Land (World).gb'),
      ).toBeInTheDocument();
      expect(within(group).getByText('Stays')).toBeInTheDocument();
      expect(
        within(group).getByText(
          '→ _Duplicados/Super Mario Land (World) (1).gb',
        ),
      ).toBeInTheDocument();
      // The proposal settles the keeper with the rules: nothing to pick,
      // so a tie does not invite a pick either.
      expect(
        within(group).getByText('They are identical; the oldest stays.'),
      ).toBeInTheDocument();
      expect(within(group).queryByRole('radio')).not.toBeInTheDocument();
      expect(within(group).queryByRole('checkbox')).not.toBeInTheDocument();
      expect(
        within(section).getByText(
          'Extra copies are set aside in _Duplicados, beside the pile: they are not copied into the library.',
        ),
      ).toBeInTheDocument();
      // Ordered between the review buckets and what stays in place.
      const review = screen
        .getByRole('heading', { name: 'To review' })
        .closest('section')!;
      const left = screen
        .getByRole('heading', { name: 'Staying where they are' })
        .closest('section')!;
      expect(
        review.compareDocumentPosition(section) &
          Node.DOCUMENT_POSITION_FOLLOWING,
      ).toBeTruthy();
      expect(
        section.compareDocumentPosition(left) &
          Node.DOCUMENT_POSITION_FOLLOWING,
      ).toBeTruthy();
    });
  });
});
