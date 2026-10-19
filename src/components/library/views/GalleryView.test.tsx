import { act, fireEvent, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi,
  type Mock,
} from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend, type Backend } from '@/ipc/backend';
import { FIXTURE_NOW, generateLibrary } from '@/ipc/mock/fixture';
import { createMockBackend, MOCK_LIBRARIES } from '@/ipc/mock/mockBackend';
import { MEDIA, REGIONS, type EntryView, type Media } from '@/ipc/types';
import { clearArtCache } from '@/lib/artCache';
import { groupEntries } from '@/lib/grouping';
import { DEFAULT_PREFS, type GroupBy } from '@/lib/viewPrefs';
import { useViewStore } from '@/stores/viewStore';
import { stubLayout } from '@/test/layout';

import { GALLERY_SETTLE_MS, GalleryView } from './GalleryView';

const big = generateLibrary({
  seed: 12,
  count: 12_000,
  root: '/mock/Big',
  generation: 1,
  now: FIXTURE_NOW,
});

function Harness({
  entries: input,
  groupBy = 'none',
  art = 'cover',
  initial = null,
  onOpen = () => {},
}: {
  entries: readonly EntryView[];
  groupBy?: GroupBy;
  art?: Media;
  initial?: number | null;
  onOpen?: (id: number) => void;
}) {
  const [selected, setSelected] = useState<number | null>(initial);
  const { ordered: entries, sections } = groupEntries(input, groupBy, {
    systems: big.systems,
    regionOrder: REGIONS,
    artMedia: MEDIA,
  });
  return (
    <I18nProvider locale="en">
      <button type="button">before</button>
      <GalleryView
        entries={entries}
        sections={sections}
        prefs={{ ...DEFAULT_PREFS, view: 'gallery', groupBy, art }}
        onPreview={() => {}}
        systems={big.systems}
        generation={1}
        selectedId={selected}
        onSelect={setSelected}
        onOpen={(id) => {
          setSelected(id);
          onOpen(id);
        }}
      />
      <button type="button">after</button>
    </I18nProvider>
  );
}

const strip = () => screen.getByRole('listbox', { name: 'Filmstrip' });
const selectedOption = () =>
  within(strip())
    .getAllByRole('option')
    .find((o) => o.getAttribute('aria-selected') === 'true');

describe('GalleryView', () => {
  let restore: () => void;
  let readArt: Mock<Backend['readArt']>;
  beforeEach(async () => {
    restore = stubLayout(1280, 800);
    const backend = createMockBackend({ delayMs: 0, now: FIXTURE_NOW });
    // The mock serves art for the library it has open.
    await backend.openLibrary(MOCK_LIBRARIES[0].path);
    readArt = vi.fn<Backend['readArt']>(backend.readArt);
    setBackend({ ...backend, readArt });
    let n = 0;
    vi.spyOn(URL, 'createObjectURL').mockImplementation(() => `blob:${++n}`);
    vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {});
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
    restore();
    clearArtCache();
    setBackend(null);
  });

  it('is a horizontal filmstrip listbox inside a named group', () => {
    render(<Harness entries={big.entries.slice(0, 20)} />);
    expect(
      screen.getByRole('group', { name: 'Gallery view' }),
    ).toBeInTheDocument();
    expect(strip()).toHaveAttribute('aria-orientation', 'horizontal');
  });

  it('renders fewer than 40 options for 12,000 entries', () => {
    render(<Harness entries={big.entries} initial={big.entries[6_000]!.id} />);
    const options = within(strip()).getAllByRole('option');
    expect(options.length).toBeGreaterThan(0);
    expect(options.length).toBeLessThan(40);
    // The active option exists so aria-activedescendant resolves.
    const active = strip().getAttribute('aria-activedescendant');
    expect(document.getElementById(active!)).not.toBeNull();
  });

  it('asks for the large art once after fast arrowing', async () => {
    vi.useFakeTimers();
    const entries = big.entries
      .filter((e) => e.art.title === 'present')
      .slice(0, 30);
    render(<Harness entries={entries} art="title" />);
    const listbox = strip();
    act(() => listbox.focus());
    for (let i = 0; i < 5; i++) {
      fireEvent.keyDown(listbox, { key: 'ArrowRight' });
      act(() => vi.advanceTimersByTime(30));
    }
    // Still moving: nothing asked for yet.
    expect(readArt.mock.calls.filter((c) => c[2] === 'title')).toHaveLength(0);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(GALLERY_SETTLE_MS + 10);
    });
    vi.useRealTimers();
    const titles = readArt.mock.calls.filter((c) => c[2] === 'title');
    expect(titles).toHaveLength(1);
    expect(titles[0]![1]).toBe(entries[4]!.id);
    expect(
      await screen.findByRole('img', {
        name: `${entries[4]!.name}, Title screen`,
      }),
    ).toHaveAttribute('src');
  });

  it('shows art already cached at once, without waiting or asking again', async () => {
    const user = userEvent.setup();
    const entries = big.entries
      .filter((e) => e.art.cover === 'present')
      .slice(0, 5);
    render(<Harness entries={entries} initial={entries[0]!.id} />);
    await screen.findByRole('img', { name: `${entries[0]!.name}, Cover` });
    act(() => strip().focus());
    await user.keyboard('{ArrowRight}');
    await screen.findByRole('img', { name: `${entries[1]!.name}, Cover` });
    const asked = readArt.mock.calls.filter((c) => c[2] === 'cover').length;
    await user.keyboard('{ArrowLeft}');
    // Back on the first game before the settle delay: its image is there.
    expect(
      screen.getByRole('img', { name: `${entries[0]!.name}, Cover` }),
    ).toHaveAttribute('src');
    expect(readArt.mock.calls.filter((c) => c[2] === 'cover').length).toBe(
      asked,
    );
  });

  it('scrolls the strip sideways with a vertical wheel', () => {
    render(<Harness entries={big.entries.slice(0, 200)} />);
    const listbox = strip();
    fireEvent.wheel(listbox, { deltaY: 120, deltaX: 0 });
    expect(listbox.scrollLeft).toBe(120);
    fireEvent.wheel(listbox, { deltaY: 0, deltaX: 40 });
    expect(listbox.scrollLeft).toBe(120);
  });

  it('labels the large image "{name}, {media}"', async () => {
    const entry = big.entries.find((e) => e.art.cover === 'present')!;
    render(<Harness entries={[entry]} initial={entry.id} />);
    const img = await screen.findByRole('img', {
      name: `${entry.name}, Cover`,
    });
    expect(img.tagName).toBe('IMG');
  });

  it('switches the large art with the tabs', async () => {
    const user = userEvent.setup();
    const entry = big.entries.find(
      (e) => e.art.cover === 'present' && e.art.screenshot === 'missing',
    )!;
    render(<Harness entries={[entry]} initial={entry.id} />);
    await user.click(screen.getByRole('tab', { name: 'Screenshot' }));
    expect(
      screen.getByRole('img', { name: `${entry.name}, No screenshot` }),
    ).toBeInTheDocument();
  });

  it('moves with ←/→ and ignores ↑/↓', async () => {
    const user = userEvent.setup();
    const entries = big.entries.slice(0, 10);
    render(<Harness entries={entries} initial={entries[2]!.id} />);
    act(() => strip().focus());
    await user.keyboard('{ArrowUp}{ArrowDown}');
    expect(selectedOption()).toHaveAttribute(
      'data-entry-id',
      String(entries[2]!.id),
    );
    await user.keyboard('{ArrowRight}{ArrowRight}{ArrowLeft}');
    expect(selectedOption()).toHaveAttribute(
      'data-entry-id',
      String(entries[3]!.id),
    );
  });

  it('opens with Enter', async () => {
    const user = userEvent.setup();
    const onOpen = vi.fn();
    const entries = big.entries.slice(0, 10);
    render(
      <Harness entries={entries} initial={entries[1]!.id} onOpen={onOpen} />,
    );
    act(() => strip().focus());
    await user.keyboard('{Enter}');
    expect(onOpen).toHaveBeenCalledWith(entries[1]!.id);
  });

  it('keeps Tab order tabs → strip → metadata, with no trap', async () => {
    const user = userEvent.setup();
    const entries = big.entries.slice(0, 10);
    render(<Harness entries={entries} initial={entries[0]!.id} />);
    screen.getByRole('button', { name: 'before' }).focus();
    await user.tab();
    expect(screen.getByRole('tab', { name: 'Cover' })).toHaveFocus();
    await user.tab();
    expect(strip()).toHaveFocus();
    await user.tab();
    expect(
      screen.getByRole('button', { name: 'Fill this game' }),
    ).toHaveFocus();
    await user.tab();
    expect(screen.getByRole('button', { name: 'after' })).toHaveFocus();
  });

  it('describes the first option of each group and hides the markers', () => {
    const entries = big.entries.slice(0, 200);
    render(<Harness entries={entries} groupBy="system" />);
    const first = within(strip()).getAllByRole('option')[0]!;
    const desc = first.getAttribute('aria-describedby');
    expect(desc).toBeTruthy();
    expect(document.getElementById(desc!)?.textContent).toBeTruthy();
    const marker = strip().querySelector('.gallery__marker');
    expect(marker).toHaveAttribute('aria-hidden', 'true');
    expect(within(strip()).getAllByRole('option')[1]).not.toHaveAttribute(
      'aria-describedby',
    );
  });

  it('shows the facts and opens the fill wizard on the game', async () => {
    const user = userEvent.setup();
    useViewStore.setState({ overlay: 'none' });
    const entry = big.entries[0]!;
    render(<Harness entries={[entry]} initial={entry.id} />);
    const meta = screen.getByRole('region', { name: 'Preview' });
    expect(
      within(meta).getByRole('heading', { name: entry.name }),
    ).toBeInTheDocument();
    expect(within(meta).getByText(`.${entry.ext}`)).toBeInTheDocument();
    expect(within(meta).getAllByRole('listitem')).toHaveLength(3);
    await user.click(
      within(meta).getByRole('button', { name: 'Fill this game' }),
    );
    expect(useViewStore.getState().overlay).toBe('wizard');
  });
});
