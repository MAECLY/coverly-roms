import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import App from '@/App';
import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend } from '@/ipc/backend';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import {
  createMockBackend,
  MOCK_LIBRARIES,
  type MockBackend,
} from '@/ipc/mock/mockBackend';
import type { EntryView, Media } from '@/ipc/types';
import { resetLibraryStore, useLibraryStore } from '@/stores/libraryStore';
import { useSelectionStore } from '@/stores/selectionStore';
import { useViewStore } from '@/stores/viewStore';
import { stubLayout } from '@/test/layout';

import { AUTO_LOAD_DELAY_MS } from './ArtWorkspace';

let backend: MockBackend;

async function setup(locale: 'en' | 'es' = 'en') {
  backend = createMockBackend({ delayMs: 0, now: FIXTURE_NOW });
  setBackend(backend);
  resetLibraryStore();
  await useLibraryStore.getState().open(MOCK_LIBRARIES[1].path);
  render(
    <I18nProvider locale={locale}>
      <App />
    </I18nProvider>,
  );
  return useLibraryStore.getState().view!;
}

/** Records the media of every automatic candidate lookup. */
function spyCandidates(): Media[] {
  const calls: Media[] = [];
  const original = backend.entryCandidates.bind(backend);
  backend.entryCandidates = (generation, id, media) => {
    calls.push(media);
    return original(generation, id, media);
  };
  return calls;
}

function show(entry: EntryView) {
  act(() => {
    useSelectionStore.getState().select(entry.id);
    useViewStore.getState().openDrawer();
  });
  return screen.getByRole('complementary', { name: entry.name });
}

describe('Detail drawer', () => {
  let restore: () => void;
  beforeEach(() => {
    restore = stubLayout(1280, 800);
  });
  afterEach(() => {
    restore();
    setBackend(null);
  });

  it('opens from a click on a tile with file facts', async () => {
    const view = await setup();
    const tile = screen.getAllByRole('gridcell')[0]!;
    await userEvent.click(tile);
    const first = view.entries.find((e) => `entry-${e.id}` === tile.id)!;
    const drawer = screen.getByRole('complementary', { name: first.name });
    expect(within(drawer).getByText(first.rel_path)).toBeInTheDocument();
    expect(within(drawer).getByText(`.${first.ext}`)).toBeInTheDocument();
    expect(
      within(drawer).getByText(view.systems[first.system]!.name),
    ).toBeInTheDocument();
  });

  it('shows the system icon beside the name', async () => {
    const view = await setup();
    const entry = view.entries[0]!;
    const system = view.systems[entry.system]!;
    const drawer = show(entry);
    const name = within(drawer).getByText(system.name);
    const dd = name.closest('dd')!;
    expect(dd).not.toBeNull();
    const icon = dd.querySelector(`[data-system-icon="${system.system_id}"]`);
    expect(icon).not.toBeNull();
    expect(icon).toHaveAttribute('aria-hidden', 'true');
    expect(icon).toHaveAttribute('height', '20');
  });

  it('shows a placeholder for missing art and loads present art', async () => {
    const view = await setup();
    const entry = view.entries.find(
      (e) => e.art.cover === 'missing' && e.art.screenshot === 'present',
    )!;
    const drawer = show(entry);
    expect(within(drawer).getByText('No cover')).toBeInTheDocument();
    expect(
      await within(drawer).findAllByRole('presentation', { hidden: true }),
    ).not.toHaveLength(0);
  });

  it('identifies on demand', async () => {
    const view = await setup();
    const drawer = show(view.entries[0]!);
    await userEvent.click(
      within(drawer).getByRole('button', { name: 'Identify' }),
    );
    expect(await within(drawer).findByText('Content hash')).toBeInTheDocument();
    expect(within(drawer).getByText('100%')).toBeInTheDocument();
  });

  it('says when identification was offline', async () => {
    const view = await setup();
    const drawer = show(view.entries[2]!);
    await userEvent.click(
      within(drawer).getByRole('button', { name: 'Identify' }),
    );
    expect(
      await within(drawer).findByText(
        'Offline: identified by its file name only.',
      ),
    ).toBeInTheDocument();
    expect(
      within(drawer).getByText('Not found in the databases'),
    ).toBeInTheDocument();
  });

  it('reveals the file from the header and offers every art action', async () => {
    const view = await setup();
    const entry = view.entries[5]!;
    const drawer = show(entry);
    const tabs = within(drawer).getAllByRole('tab');
    expect(tabs.map((t) => t.getAttribute('aria-label'))).toEqual([
      'Cover',
      'Screenshot',
      'Title screen',
    ]);
    const active = tabs.find(
      (t) => t.getAttribute('aria-selected') === 'true',
    )!;
    const media = (['cover', 'screenshot', 'title'] as const)[
      tabs.indexOf(active)
    ]!;
    expect(
      within(drawer).getByRole('searchbox', { name: 'Title to search' }),
    ).toBeEnabled();
    expect(
      within(drawer).getByRole('button', { name: 'From file…' }),
    ).toBeEnabled();
    expect(
      within(drawer).getByRole('button', { name: 'Remove' }),
    ).toHaveProperty('disabled', entry.art[media] !== 'present');
    await userEvent.click(
      within(drawer).getByRole('button', {
        name: /Reveal in Finder|Show in Explorer|Show in folder/,
      }),
    );
    expect(backend.revealed).toEqual([entry.id]);
  });

  it('shows the source of each candidate and why the first one is chosen', async () => {
    const view = await setup();
    const entry = view.entries.find((e) => e.art.cover === 'missing')!;
    const drawer = show(entry);
    // No click: the first missing art loads its candidates on its own.
    const group = await within(drawer).findByRole('group', {
      name: 'Candidates',
    });
    expect(
      within(group).getAllByText('libretro-thumbnails').length,
    ).toBeGreaterThan(0);
    const [first, second] = within(group).getAllByRole('button');
    expect(
      within(first!).getByText('Why: it matches this ROM better.'),
    ).toBeInTheDocument();
    expect(within(second!).queryByText(/^Why: /)).not.toBeInTheDocument();
  });

  it('opens on the first missing art and loads its candidates once', async () => {
    const view = await setup();
    const entry = view.entries.find(
      (e) => e.art.cover === 'present' && e.art.screenshot === 'missing',
    )!;
    const calls = spyCandidates();
    const drawer = show(entry);
    expect(
      within(drawer).getByRole('tab', { name: 'Screenshot' }),
    ).toHaveAttribute('aria-selected', 'true');
    expect(within(drawer).getByText('Looking for images…')).toHaveAttribute(
      'role',
      'status',
    );
    await within(drawer).findByRole('group', { name: 'Candidates' });
    expect(calls).toEqual(['screenshot']);
  });

  it('waits to be asked when no art is missing', async () => {
    const view = await setup();
    const entry = view.entries.find(
      (e) =>
        e.art.cover === 'present' &&
        e.art.screenshot === 'present' &&
        e.art.title === 'present',
    )!;
    const calls = spyCandidates();
    const drawer = show(entry);
    expect(within(drawer).getByRole('tab', { name: 'Cover' })).toHaveAttribute(
      'aria-selected',
      'true',
    );
    await act(() => new Promise((r) => setTimeout(r, AUTO_LOAD_DELAY_MS + 50)));
    expect(calls).toEqual([]);
    await userEvent.click(
      within(drawer).getByRole('button', { name: 'Show other images' }),
    );
    await within(drawer).findByRole('group', { name: 'Candidates' });
    expect(calls).toEqual(['cover']);
  });

  it('browsing the slots with the arrow keys loads only where it stops', async () => {
    const view = await setup();
    const entry = view.entries.find((e) => e.art.cover === 'missing')!;
    const calls = spyCandidates();
    const drawer = show(entry);
    const cover = within(drawer).getByRole('tab', { name: 'Cover' });
    act(() => cover.focus());
    expect(cover).toHaveAttribute('tabindex', '0');
    await userEvent.keyboard('{ArrowRight}');
    const shot = within(drawer).getByRole('tab', { name: 'Screenshot' });
    expect(shot).toHaveFocus();
    await userEvent.keyboard('{ArrowRight}');
    const title = within(drawer).getByRole('tab', { name: 'Title screen' });
    expect(title).toHaveFocus();
    expect(title).toHaveAttribute('aria-selected', 'true');
    expect(cover).toHaveAttribute('tabindex', '-1');
    await within(drawer).findByRole('group', { name: 'Candidates' });
    expect(calls).toEqual(['title']);
    await userEvent.keyboard('{Home}');
    expect(cover).toHaveFocus();
    expect(cover).toHaveAttribute('aria-selected', 'true');
  });

  it('ignores a late answer for a slot that is no longer active', async () => {
    const view = await setup();
    const entry = view.entries.find(
      (e) => e.art.cover === 'missing' && e.art.screenshot === 'present',
    )!;
    let release!: () => void;
    const gate = new Promise<void>((r) => {
      release = r;
    });
    const calls: Media[] = [];
    const original = backend.entryCandidates.bind(backend);
    backend.entryCandidates = async (generation, id, media) => {
      calls.push(media);
      const list = await original(generation, id, media);
      if (media !== 'cover') return list;
      await gate;
      return list.map((c) => ({ ...c, name: `STALE ${c.name}` }));
    };
    const drawer = show(entry);
    await waitFor(() => expect(calls).toEqual(['cover']));
    await userEvent.click(
      within(drawer).getByRole('tab', { name: 'Screenshot' }),
    );
    await within(drawer).findByRole('group', { name: 'Candidates' });
    await act(async () => {
      release();
      await gate;
    });
    expect(calls).toEqual(['cover', 'screenshot']);
    expect(within(drawer).queryByText(/STALE/)).not.toBeInTheDocument();
    // The screenshot list stays: the late cover answer replaced nothing.
    expect(
      within(
        within(drawer).getByRole('group', { name: 'Candidates' }),
      ).getAllByRole('button'),
    ).toHaveLength(3);
    expect(
      within(drawer).getByRole('tab', { name: 'Screenshot' }),
    ).toHaveAttribute('aria-selected', 'true');
  });

  it('shows art the layout cannot store but does not offer it', async () => {
    const view = await setup();
    const entry = view.entries.find((e) => e.art.cover === 'missing')!;
    act(() =>
      useLibraryStore.setState({
        view: { ...view, profile: { ...view.profile, media: ['cover'] } },
      }),
    );
    const calls = spyCandidates();
    const drawer = show(entry);
    const shot = within(drawer).getByRole('tab', { name: 'Screenshot' });
    expect(shot).toBeDisabled();
    expect(shot).toHaveAccessibleDescription('Not in this layout');
    await userEvent.click(shot);
    expect(within(drawer).getByRole('tab', { name: 'Cover' })).toHaveAttribute(
      'aria-selected',
      'true',
    );
    await within(drawer).findByRole('group', { name: 'Candidates' });
    expect(calls).toEqual(['cover']);
  });

  it('drawer_renders_hostile_name_as_text', async () => {
    const view = await setup();
    const hostile = view.entries.find(
      (e) => e.name === '<img src=x onerror=alert(1)>',
    )!;
    const drawer = show(hostile);
    expect(
      within(drawer).getByRole('heading', { name: hostile.name }),
    ).toBeInTheDocument();
    expect(document.querySelector('img[src="x"]')).toBeNull();
  });

  it('closes with Escape and with the close button', async () => {
    const view = await setup();
    show(view.entries[0]!);
    fireEvent.keyDown(document.body, { key: 'Escape' });
    expect(screen.queryByRole('complementary')).not.toBeInTheDocument();
    const drawer = show(view.entries[0]!);
    await userEvent.click(
      within(drawer).getByRole('button', { name: 'Close details' }),
    );
    expect(screen.queryByRole('complementary')).not.toBeInTheDocument();
  });

  it('follows the keyboard selection', async () => {
    const view = await setup();
    show(view.entries[0]!);
    const grid = screen.getByRole('grid', { name: 'ROM covers' });
    fireEvent.keyDown(grid, { key: 'ArrowRight' });
    expect(
      screen.getByRole('complementary', { name: view.entries[1]!.name }),
    ).toBeInTheDocument();
  });

  it('speaks Spanish', async () => {
    const view = await setup('es');
    const drawer = show(view.entries[0]!);
    expect(
      within(drawer).getByRole('button', { name: 'Identificar' }),
    ).toBeInTheDocument();
    expect(
      within(drawer).getByRole('tab', { name: 'Portada' }),
    ).toBeInTheDocument();
    expect(
      within(drawer).getByRole('button', { name: 'Desde archivo…' }),
    ).toBeEnabled();
  });
});
