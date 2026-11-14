import { fireEvent, render, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';
import type { EntryView } from '@/ipc/types';
import { useViewStore } from '@/stores/viewStore';

import { FixedFoldersSection } from './FixedFoldersSection';

const base: EntryView = {
  id: 0,
  system: 0,
  rel_path: 'ROMS/PS4/Lantern Harbor/Lantern Harbor (USA).pkg',
  dir: 'ROMS/PS4/Lantern Harbor',
  name: 'Lantern Harbor (USA)',
  ext: 'pkg',
  size: 1,
  mtime: 1,
  regions: ['usa'],
  art: { cover: 'present', screenshot: 'missing', title: 'missing' },
};

function section(entry: EntryView) {
  const { container } = render(
    <I18nProvider locale="es">
      <FixedFoldersSection entry={entry} />
    </I18nProvider>,
  );
  return container;
}

describe('FixedFoldersSection (addon views §8.8 6, D9)', () => {
  it('is collapsed, read-only, with a count or «vacía» per folder', () => {
    const c = section({
      ...base,
      addons: { mods: 1, cheats: 2, extras: 3, saves: 0 },
    });
    const details = c.querySelector('details')!;
    expect(details).toHaveAttribute('id', 'panel-folders');
    expect(details.open).toBe(false);
    expect(details.querySelector('summary')).toHaveTextContent(
      'Mods, trucos, extras y partidas',
    );
    const cells = [...details.querySelectorAll('.panel-folders__cell')].map(
      (cell) => cell.textContent,
    );
    expect(cells).toEqual([
      'Mods1 archivo',
      'Trucos2 archivos',
      'Extras3 archivosVer extras',
      'Partidasvacía',
    ]);
    expect(
      within(details).getByText(
        'Aquí se verán las partidas de este juego, con fecha y emulador, cuando llegue el bloque 5.',
      ),
    ).toBeInTheDocument();
    const path = details.querySelector('.panel-folders__path')!;
    expect(path).toHaveClass('mono');
    expect(path).toHaveTextContent('ROMS/PS4/Lantern Harbor');
    // Nothing here writes: the one button scrolls to the Extras section.
    const buttons = within(details).getAllByRole('button');
    expect(buttons).toHaveLength(1);
    fireEvent.click(buttons[0]!);
    expect(useViewStore.getState().panelSection).toBe('extras');
    useViewStore.setState({ panelSection: null });
  });

  it('says «más de 10 000 archivos» when the count stopped', () => {
    const c = section({
      ...base,
      addons: { mods: 10_000, cheats: 0, extras: 0, saves: 0, capped: true },
    });
    expect(c.querySelector('.panel-folders__cell dd')).toHaveTextContent(
      /^más de 10[.\s\u00a0\u202f]000 archivos$/,
    );
  });

  it('names a folder game by its own folder', () => {
    const c = section({
      ...base,
      rel_path: 'ROMS/PS4/Kite Days.ps4',
      dir: 'ROMS/PS4',
      folder_root: true,
      addons: { mods: 0, cheats: 0, extras: 0, saves: 0 },
    });
    expect(c.querySelector('.panel-folders__path')).toHaveTextContent(
      'ROMS/PS4/Kite Days.ps4',
    );
  });

  it('renders nothing for an entry without fixed folders', () => {
    expect(section(base)).toBeEmptyDOMElement();
  });
});
