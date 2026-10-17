import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { OrganizeOffer } from '@/components/organize/OrganizeOffer';
import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend } from '@/ipc/backend';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import { createMockBackend } from '@/ipc/mock/mockBackend';
import type { FolderProbe } from '@/ipc/types';

const PILE: FolderProbe = {
  rom_like: 17,
  partial_downloads: 3,
  cloud_only: 0,
  sorted_folders: [],
  writable: true,
  broad_root: false,
  truncated: false,
};

function renderOffer(
  probe: FolderProbe,
  locale: 'en' | 'es' = 'en',
  defaultLayout?: string,
) {
  const handlers = {
    onOrganize: vi.fn(),
    onOpenSorted: vi.fn(),
    onPickOther: vi.fn(),
  };
  render(
    <I18nProvider locale={locale}>
      <OrganizeOffer
        probe={probe}
        defaultLayout={defaultLayout}
        {...handlers}
      />
    </I18nProvider>,
  );
  return handlers;
}

describe('OrganizeOffer', () => {
  beforeEach(() => {
    setBackend(createMockBackend({ delayMs: 0, now: FIXTURE_NOW }));
  });
  afterEach(() => setBackend(null));

  it('offers to organise a pile with its ROM count', async () => {
    const h = renderOffer(PILE);
    expect(
      screen.getByRole('heading', {
        name: "This folder doesn't follow a known layout",
      }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/It holds 17 files that look like ROMs/),
    ).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole('button', { name: 'Organise this folder…' }),
    );
    expect(h.onOrganize).toHaveBeenCalledTimes(1);
    await userEvent.click(
      screen.getByRole('button', { name: 'Choose another folder' }),
    );
    expect(h.onPickOther).toHaveBeenCalledTimes(1);
    expect(
      screen.queryByRole('button', { name: 'Open as organised by system' }),
    ).not.toBeInTheDocument();
  });

  it('speaks Spanish', () => {
    renderOffer(PILE, 'es');
    expect(
      screen.getByRole('heading', {
        name: 'Esta carpeta no sigue ningún formato conocido',
      }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Organizar esta carpeta…' }),
    ).toBeInTheDocument();
  });

  it('warns on broad personal folders and shows 5,000+ when truncated', () => {
    renderOffer({ ...PILE, rom_like: 5000, broad_root: true, truncated: true });
    expect(screen.getByText(/It holds 5,000\+ files/)).toBeInTheDocument();
    expect(
      screen.getByText(
        'This is a broad personal folder; only files that look like ROMs are proposed.',
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        'More than 5,000 files; all of them are checked when organising.',
      ),
    ).toBeInTheDocument();
  });

  it('offers to open a folder that is already sorted, with a layout', async () => {
    const h = renderOffer(
      { ...PILE, rom_like: 0, sorted_folders: ['GB', 'GBA'] },
      'en',
      'anbernic-stock',
    );
    expect(
      screen.getByText('Looks like a folder organised by system'),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: 'Organise this folder…' }),
    ).not.toBeInTheDocument();
    const select = screen.getByLabelText('Layout');
    expect(select).toHaveValue('anbernic-stock');
    await screen.findByRole('option', { name: 'muOS' });
    await userEvent.selectOptions(select, 'muos');
    await userEvent.click(
      screen.getByRole('button', { name: 'Open as organised by system' }),
    );
    expect(h.onOpenSorted).toHaveBeenCalledWith('muos');
  });
});
