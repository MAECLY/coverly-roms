import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';

import { ExtractDetail } from '@/components/organize/ExtractDetail';
import { I18nProvider } from '@/i18n/I18nProvider';
import type { DiscLayout, OrganizeItem } from '@/ipc/types';
import { discItem } from '@/test/organizeFixtures';

function renderDetail(
  item: OrganizeItem,
  notes: string[] = [],
  disc_layout?: DiscLayout,
  locale: 'en' | 'es' = 'en',
) {
  render(
    <I18nProvider locale={locale}>
      <ExtractDetail
        item={item}
        profile={disc_layout ? { disc_layout } : null}
        notes={notes}
      />
    </I18nProvider>,
  );
}

describe('ExtractDetail', () => {
  it('expand_lists_files_and_target', async () => {
    renderDetail(discItem(0));
    const toggle = screen.getByRole('button', { name: 'Files to extract' });
    expect(toggle).toHaveAttribute('aria-expanded', 'false');
    // Closed, the list is not in the DOM (thousand-row piles stay light).
    expect(screen.queryAllByRole('listitem')).toHaveLength(0);
    await userEvent.click(toggle);
    expect(toggle).toHaveAttribute('aria-expanded', 'true');
    const list = screen.getByRole('list', { name: 'Files to extract' });
    const rows = within(list).getAllByRole('listitem');
    expect(rows).toHaveLength(12);
    expect(within(rows[0]!).getByText('Next Tetris, The.cue')).toBeVisible();
    expect(within(rows[0]!).getByText('1.2 KB')).toHaveClass('font-mono');
    expect(
      within(rows[1]!).getByText('Next Tetris, The (Track 01).bin'),
    ).toBeVisible();
    // Sizes are mono and right-aligned, like a ledger column.
    const size = within(rows[1]!).getByText('300 MB');
    expect(size).toHaveClass('font-mono');
    expect(size).toHaveClass('text-right');
    expect(screen.getByText('Tracks go straight into ROMS/PS')).toBeVisible();
  });

  it('names the per-game folder and the playlist', async () => {
    renderDetail(
      discItem(0, {
        target: 'ROMS/PS/Next Tetris, The/Next Tetris, The.cue',
        transfer: {
          ...(discItem(0).transfer as Extract<
            OrganizeItem['transfer'],
            { kind: 'extract' }
          >),
          m3u: 'ROMS/PS/Next Tetris, The/Next Tetris, The.m3u',
        },
      }),
      [],
      { kind: 'game-folder', m3u_for_multi: true },
    );
    await userEvent.click(
      screen.getByRole('button', { name: 'Files to extract' }),
    );
    expect(
      screen.getByText('Each game in its own folder: ROMS/PS/Next Tetris, The'),
    ).toBeVisible();
    expect(
      screen.getByText('Next Tetris, The.m3u is created for disc swapping'),
    ).toBeVisible();
  });

  it('notes_render_container_mismatch_and_twin_sheets', async () => {
    renderDetail(discItem(0), [
      'It is a 7z archive named .zip',
      'Has a .gdi and a .cue for the same tracks; some menus will list the game twice',
    ]);
    await userEvent.click(
      screen.getByRole('button', { name: 'Files to extract' }),
    );
    expect(screen.getByText('It is a 7z archive named .zip')).toBeVisible();
    expect(
      screen.getByText(
        'Has a .gdi and a .cue for the same tracks; some menus will list the game twice',
      ),
    ).toBeVisible();
  });

  it('names each file as written to the card', async () => {
    const item = discItem(0);
    const t = item.transfer as Extract<
      OrganizeItem['transfer'],
      { kind: 'extract' }
    >;
    renderDetail({
      ...item,
      transfer: {
        ...t,
        entries: t.entries.map((e) => ({ ...e, name: `Disc/${e.name}` })),
      },
    });
    await userEvent.click(
      screen.getByRole('button', { name: 'Files to extract' }),
    );
    const cue = screen.getByText('Next Tetris, The.cue');
    expect(cue).toHaveAttribute('title', 'Disc/Next Tetris, The.cue');
  });

  it('uses the Spanish copy and separators', async () => {
    renderDetail(discItem(0), [], undefined, 'es');
    await userEvent.click(
      screen.getByRole('button', { name: 'Archivos que se extraen' }),
    );
    expect(screen.getByText('1,2 KB')).toBeVisible();
    expect(
      screen.getByText('Las pistas van directamente en ROMS/PS'),
    ).toBeVisible();
  });

  it('renders nothing for an item that moves as it is', () => {
    const { container } = render(
      <I18nProvider locale="en">
        <ExtractDetail
          item={{ ...discItem(0), transfer: { kind: 'as-is' } }}
          profile={null}
          notes={[]}
        />
      </I18nProvider>,
    );
    expect(container).toBeEmptyDOMElement();
  });
});
