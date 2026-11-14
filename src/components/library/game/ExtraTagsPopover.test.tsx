import { fireEvent, render, screen, within } from '@testing-library/react';
import { useState } from 'react';
import { describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';
import { MAX_TAGS } from '@/lib/extras';

import { ExtraTagsPopover } from './ExtraTagsPopover';

function Harness({
  tags,
  suggestions = [],
  onSave,
}: {
  tags: string[];
  suggestions?: string[];
  onSave(tags: string[]): void;
}) {
  const [open, setOpen] = useState(true);
  return (
    <I18nProvider locale="es">
      <ExtraTagsPopover
        name="03 Leviathan.flac"
        tags={tags}
        suggestions={suggestions}
        open={open}
        onOpenChange={setOpen}
        onSave={onSave}
      >
        <button type="button">fila</button>
      </ExtraTagsPopover>
    </I18nProvider>
  );
}

const input = () => screen.getByRole('textbox', { name: 'Añadir etiqueta' });

describe('ExtraTagsPopover (extras spec §8.3, D20)', () => {
  it('names the file, shows its tags as removable chips and focuses the input', () => {
    render(<Harness tags={['tema del jefe final']} onSave={vi.fn()} />);
    const pop = screen.getByRole('dialog');
    expect(
      within(pop).getByText('Etiquetas de 03 Leviathan.flac'),
    ).toBeInTheDocument();
    expect(
      within(pop).getByRole('button', {
        name: 'Quitar la etiqueta tema del jefe final',
      }),
    ).toBeInTheDocument();
    expect(input()).toHaveFocus();
  });

  it('Enter adds a chip (trimmed, no case duplicates); Backspace on empty removes the last', () => {
    const onSave = vi.fn();
    render(<Harness tags={['Jefe']} onSave={onSave} />);
    fireEvent.change(input(), { target: { value: '  jefe ' } });
    fireEvent.keyDown(input(), { key: 'Enter' });
    expect(
      screen.getAllByRole('button', { name: /^Quitar la etiqueta/ }),
    ).toHaveLength(1);
    fireEvent.change(input(), { target: { value: ' orquestal ' } });
    fireEvent.keyDown(input(), { key: 'Enter' });
    expect(input()).toHaveValue('');
    expect(
      screen.getByRole('button', { name: 'Quitar la etiqueta orquestal' }),
    ).toBeInTheDocument();
    fireEvent.keyDown(input(), { key: 'Backspace' });
    fireEvent.keyDown(input(), { key: 'Backspace' });
    fireEvent.keyDown(input(), { key: 'Escape' });
    expect(onSave).toHaveBeenCalledWith([]);
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('offers the suggestions it does not have yet', () => {
    const onSave = vi.fn();
    render(
      <Harness
        tags={['ambiente']}
        suggestions={['Ambiente', 'créditos', 'lo-fi']}
        onSave={onSave}
      />,
    );
    const pop = screen.getByRole('dialog');
    expect(within(pop).getByText('Sugerencias')).toBeInTheDocument();
    expect(within(pop).queryByRole('button', { name: 'Ambiente' })).toBeNull();
    fireEvent.click(within(pop).getByRole('button', { name: 'lo-fi' }));
    expect(within(pop).queryByRole('button', { name: 'lo-fi' })).toBeNull();
    fireEvent.keyDown(input(), { key: 'Escape' });
    expect(onSave).toHaveBeenCalledWith(['ambiente', 'lo-fi']);
  });

  it('saves nothing when nothing changed', () => {
    const onSave = vi.fn();
    render(<Harness tags={['a']} onSave={onSave} />);
    fireEvent.keyDown(input(), { key: 'Escape' });
    expect(onSave).not.toHaveBeenCalled();
  });

  it('keeps its keys from the panel and says when the file is full', () => {
    const outer = vi.fn();
    window.addEventListener('keydown', outer);
    const full = Array.from({ length: MAX_TAGS }, (_, i) => `t${i}`);
    render(<Harness tags={full} onSave={vi.fn()} />);
    expect(input()).toBeDisabled();
    const note = screen
      .getByRole('dialog')
      .querySelector('.extras-tagpop__note')!;
    expect(note).toHaveTextContent('Como mucho 32 etiquetas por archivo.');
    expect(note.querySelector('.mono')).toHaveTextContent('32');
    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'i' });
    expect(outer).not.toHaveBeenCalled();
    window.removeEventListener('keydown', outer);
  });
});
