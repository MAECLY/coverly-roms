import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useRef, useState } from 'react';
import { describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';
import type { Locale } from '@/i18n/core';
import type { LargeTransferDetail } from '@/ipc/types';
import type { HugeAnswer } from '@/stores/organizeStore';

import { HugeItemDialog } from './HugeItemDialog';

/** The huge pile of the mock: 120 GB free on «Juegos», 6,92 GB of rest. */
const HUGE: LargeTransferDetail = {
  bytes: 166_230_181_124,
  items: 6,
  copies: true,
  free: 120_000_000_000,
  volume: 'Juegos',
  huge: [
    {
      item: 0,
      name: 'Bloodborne',
      system: 'ps4',
      bytes: 52_400_000_000,
      extract: false,
      followers: [],
    },
    {
      item: 1,
      name: "Demon's Souls (BLUS-30443).zip",
      system: 'ps3',
      bytes: 7_410_181_124,
      extract: true,
      followers: [],
    },
    {
      item: 2,
      name: 'Ghost of Tsushima',
      system: 'ps4',
      bytes: 61_200_000_000,
      extract: false,
      followers: [],
    },
  ],
  rest: 6_920_000_000,
};

/** The dialog driven like the store does: «Sí» accepts, «No» skips. */
function Harness({
  detail,
  onAnswer,
}: {
  detail: LargeTransferDetail;
  onAnswer(a: HugeAnswer): void;
}) {
  const apply = useRef<HTMLButtonElement>(null);
  const [cursor, setCursor] = useState(0);
  const [accepted, setAccepted] = useState<number[]>([]);
  return (
    <>
      <button ref={apply} type="button">
        Organizar
      </button>
      {cursor < detail.huge.length && (
        <HugeItemDialog
          detail={detail}
          cursor={cursor}
          accepted={accepted}
          returnFocus={apply}
          onAnswer={(a) => {
            onAnswer(a);
            const h = detail.huge[cursor]!;
            if (a === 'yes') setAccepted((x) => [...x, h.item]);
            setCursor((c) => (a === 'all' ? detail.huge.length : c + 1));
          }}
        />
      )}
    </>
  );
}

function renderDialog(detail = HUGE, locale: Locale = 'es') {
  const onAnswer = vi.fn();
  const r = render(
    <I18nProvider locale={locale}>
      <Harness detail={detail} onAnswer={onAnswer} />
    </I18nProvider>,
  );
  return { ...r, onAnswer };
}

const dialog = () => screen.getByRole('alertdialog');

describe('HugeItemDialog', () => {
  it('asks the first item with its size, the free space after it and the progress', () => {
    renderDialog();
    expect(dialog()).toHaveAccessibleName('¿Copiar Bloodborne?');
    // (6,92 + 52,4) GB + 1 % = 59,91 GB out of 120 GB.
    expect(dialog()).toHaveAccessibleDescription(
      'Ocupa 52,4 GB. Quedarán 60,09 GB libres en Juegos. Elemento 1 de 3',
    );
    const mono = [...dialog().querySelectorAll('.mono')].map(
      (n) => n.textContent,
    );
    // Sizes and counts in mono; the name and the volume are plain.
    expect(mono).toEqual(['52,4 GB', '60,09 GB', '1', '3']);
    const buttons = within(dialog())
      .getAllByRole('button')
      .map((b) => b.textContent);
    expect(buttons).toEqual(['No', 'Sí a todo', 'Sí']);
  });

  it('focuses «No» on every item and keeps one dialog for the sequence', async () => {
    const user = userEvent.setup();
    const { onAnswer } = renderDialog();
    const first = dialog();
    expect(screen.getByRole('button', { name: 'No' })).toHaveFocus();
    await user.click(screen.getByRole('button', { name: 'Sí' }));
    expect(onAnswer).toHaveBeenLastCalledWith('yes');
    // Same node, new content: an extraction now.
    expect(dialog()).toBe(first);
    expect(dialog()).toHaveAccessibleName(
      "¿Extraer Demon's Souls (BLUS-30443).zip?",
    );
    expect(screen.getByRole('button', { name: 'No' })).toHaveFocus();
    // The free space goes down with the item accepted.
    expect(dialog()).toHaveAccessibleDescription(
      'Ocupa 7,41 GB. Quedarán 52,6 GB libres en Juegos. Elemento 2 de 3',
    );
  });

  it('Escape is «No» for this item only', async () => {
    const user = userEvent.setup();
    const { onAnswer } = renderDialog();
    await user.keyboard('{Escape}');
    expect(onAnswer).toHaveBeenCalledWith('no');
    expect(dialog()).toHaveAccessibleName(
      "¿Extraer Demon's Souls (BLUS-30443).zip?",
    );
  });

  it('an item that does not fit: «Sí» and «Sí a todo» stay announced but do nothing', async () => {
    const user = userEvent.setup();
    const { onAnswer } = renderDialog();
    await user.click(screen.getByRole('button', { name: 'Sí' }));
    await user.click(screen.getByRole('button', { name: 'Sí' }));
    // 6,92 + 52,4 + 7,41 + 61,2 GB (+1 %) = 129,21 GB > 120 GB.
    expect(dialog()).toHaveAccessibleName('¿Copiar Ghost of Tsushima?');
    expect(dialog()).toHaveAccessibleDescription(
      'No cabe en Juegos: faltan 9,21 GB. Elemento 3 de 3',
    );
    const yes = screen.getByRole('button', { name: 'Sí' });
    const all = screen.getByRole('button', { name: 'Sí a todo' });
    for (const b of [yes, all]) {
      expect(b).toHaveAttribute('aria-disabled', 'true');
      expect(b).toHaveAccessibleDescription(
        'No cabe en Juegos: faltan 9,21 GB.',
      );
    }
    onAnswer.mockClear();
    await user.click(yes);
    await user.click(all);
    expect(onAnswer).not.toHaveBeenCalled();
    await user.click(screen.getByRole('button', { name: 'No' }));
    expect(onAnswer).toHaveBeenCalledWith('no');
  });

  it('«Sí a todo» answers the current item and every later one', async () => {
    const user = userEvent.setup();
    const { onAnswer } = renderDialog();
    await user.click(screen.getByRole('button', { name: 'Sí a todo' }));
    expect(onAnswer).toHaveBeenCalledTimes(1);
    expect(onAnswer).toHaveBeenCalledWith('all');
    expect(screen.queryByRole('alertdialog')).toBeNull();
    // Focus goes back to the apply button.
    expect(screen.getByRole('button', { name: 'Organizar' })).toHaveFocus();
  });

  it('without the free space: the size alone, and English titles by mode', () => {
    renderDialog(
      {
        ...HUGE,
        copies: false,
        free: null,
        huge: [HUGE.huge[0]!],
      },
      'en',
    );
    expect(dialog()).toHaveAccessibleName('Move Bloodborne to another drive?');
    expect(dialog()).toHaveAccessibleDescription(
      'It takes 52.4 GB. Item 1 of 1',
    );
    expect(
      within(dialog())
        .getAllByRole('button')
        .map((b) => b.textContent),
    ).toEqual(['No', 'Yes to all', 'Yes']);
  });
});
