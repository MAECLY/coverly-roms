import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { RevealButton } from '@/components/dedupe/RevealButton';
import { TooltipProvider } from '@/components/ui/tooltip';
import { I18nProvider } from '@/i18n/I18nProvider';

const MAC =
  'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko)';
const WIN =
  'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko)';
const LINUX = 'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36';

function asOs(ua: string) {
  vi.spyOn(navigator, 'userAgent', 'get').mockReturnValue(ua);
}

function wrap(ui: React.ReactNode) {
  return render(
    <I18nProvider locale="es">
      <TooltipProvider delayDuration={0}>{ui}</TooltipProvider>
    </I18nProvider>,
  );
}

const PATH = '/Volumes/SD/ROMS/GB/Tetris (Japan).gb';

afterEach(() => vi.restoreAllMocks());

describe('RevealButton', () => {
  it('names the copy in a whole sentence for Finder, with the OS label as tooltip', async () => {
    asOs(MAC);
    const onReveal = vi.fn();
    wrap(
      <RevealButton path={PATH} name="Tetris (Japan).gb" onReveal={onReveal} />,
    );
    const button = screen.getByRole('button', {
      name: 'Mostrar Tetris (Japan).gb en Finder',
    });
    expect(button).toHaveAttribute('data-reveal');
    expect(button).not.toHaveAttribute('aria-keyshortcuts');
    await userEvent.hover(button);
    expect(
      (await screen.findAllByText('Mostrar en Finder')).length,
    ).toBeGreaterThan(0);
    await userEvent.click(button);
    expect(onReveal).toHaveBeenCalledWith(PATH);
  });

  it('says Explorer on Windows and advertises the chord when asked', () => {
    asOs(WIN);
    wrap(
      <RevealButton
        path={PATH}
        name="Tetris (Japan).gb"
        onReveal={vi.fn()}
        shortcut
      />,
    );
    const button = screen.getByRole('button', {
      name: 'Mostrar Tetris (Japan).gb en el Explorador',
    });
    expect(button).toHaveAttribute('aria-keyshortcuts', 'Shift+Alt+R');
  });

  it('names a folder with the folder sentence', () => {
    asOs(LINUX);
    wrap(
      <RevealButton
        path="/Volumes/SD/_Duplicados"
        name="_Duplicados"
        kind="folder"
        onReveal={vi.fn()}
      />,
    );
    expect(
      screen.getByRole('button', { name: 'Mostrar _Duplicados en la carpeta' }),
    ).toBeInTheDocument();
  });

  it('renders nothing when the copy has no place (sent to the Trash)', () => {
    asOs(MAC);
    const { container } = wrap(
      <RevealButton path={null} name="Tetris.gb" onReveal={vi.fn()} />,
    );
    expect(container.querySelector('button')).toBeNull();
  });
});
