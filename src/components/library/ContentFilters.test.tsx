import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';
import type { Locale } from '@/i18n/core';
import type { ContentKey } from '@/lib/filters';
import { useFilterStore } from '@/stores/filterStore';

import { ContentFilters } from './ContentFilters';

const COUNTS: Record<ContentKey, number> = {
  updates: 3,
  missing: 1,
  dlc: 4,
  demos: 2,
};

function row(
  props: { counts?: Record<ContentKey, number>; shown?: number } = {},
  locale: Locale = 'es',
) {
  const ui = (shown: number) => (
    <I18nProvider locale={locale}>
      <ContentFilters counts={props.counts ?? COUNTS} shown={shown} />
    </I18nProvider>
  );
  const view = render(ui(props.shown ?? 9));
  return { ...view, setShown: (n: number) => view.rerender(ui(n)) };
}

const live = () =>
  document.querySelector('[aria-live="polite"]')?.textContent ?? '';

describe('ContentFilters (D14)', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => {
    vi.useRealTimers();
    act(() => useFilterStore.getState().reset());
  });

  it('a labelled group of four chips, each with its count', () => {
    row();
    expect(
      screen.getByRole('group', { name: 'Mostrar juegos que' }),
    ).toBeInTheDocument();
    const chips = screen.getAllByRole('button');
    expect(chips.map((c) => c.textContent)).toEqual([
      'Tiene actualizaciones3',
      'Le falta la última actualización1',
      'Tiene DLC4',
      'Solo demos2',
    ]);
    for (const chip of chips)
      expect(chip).toHaveAttribute('aria-pressed', 'false');
  });

  it('a press toggles the filter in the store', () => {
    row();
    fireEvent.click(screen.getByRole('button', { name: /Tiene DLC/ }));
    expect(useFilterStore.getState().filters.content.dlc).toBe(true);
    expect(screen.getByRole('button', { name: /Tiene DLC/ })).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    fireEvent.click(screen.getByRole('button', { name: /Solo demos/ }));
    expect(useFilterStore.getState().filters.content).toMatchObject({
      dlc: true,
      demos: true,
    });
    fireEvent.click(screen.getByRole('button', { name: /Tiene DLC/ }));
    expect(useFilterStore.getState().filters.content.dlc).toBe(false);
  });

  it('«Le falta…» is described by the visible in-library hint', () => {
    row();
    const chip = screen.getByRole('button', { name: /Le falta/ });
    const hint = screen.getByText(
      'Compara solo dentro de tu biblioteca hasta que haya lista de títulos.',
    );
    expect(hint).toBeVisible();
    expect(chip.getAttribute('aria-describedby')).toBe(hint.id);
  });

  it('announces the result 500 ms after a press, never on search typing', () => {
    const { setShown } = row({ shown: 9 });
    fireEvent.click(screen.getByRole('button', { name: /Tiene DLC/ }));
    setShown(2);
    act(() => vi.advanceTimersByTime(499));
    expect(live()).toBe('');
    act(() => vi.advanceTimersByTime(1));
    expect(live()).toBe('2 juegos con estos filtros.');

    fireEvent.click(screen.getByRole('button', { name: /Solo demos/ }));
    setShown(0);
    act(() => vi.advanceTimersByTime(500));
    expect(live()).toBe('Ningún juego cumple estos filtros.');

    // The search narrows the games too, but says nothing here.
    act(() => useFilterStore.getState().setQuery('harbor'));
    setShown(1);
    act(() => vi.advanceTimersByTime(1000));
    expect(live()).toBe('Ningún juego cumple estos filtros.');
  });

  it('one game is said in the singular', () => {
    const { setShown } = row();
    fireEvent.click(screen.getByRole('button', { name: /Solo demos/ }));
    setShown(1);
    act(() => vi.advanceTimersByTime(500));
    expect(live()).toBe('1 juego con estos filtros.');
  });

  it('a chip at 0 stays enabled', () => {
    row({ counts: { ...COUNTS, missing: 0 } });
    expect(screen.getByRole('button', { name: /Le falta/ })).toBeEnabled();
  });

  it('is hidden when the library has no content, unless a chip is on', () => {
    const none = { updates: 0, missing: 0, dlc: 0, demos: 0 };
    const { container, unmount } = row({ counts: none });
    expect(container).toBeEmptyDOMElement();
    unmount();
    act(() => useFilterStore.getState().toggleContent('dlc'));
    row({ counts: none });
    expect(screen.getByRole('button', { name: /Tiene DLC/ })).toHaveAttribute(
      'aria-pressed',
      'true',
    );
  });

  it('speaks English', () => {
    const { setShown } = row({}, 'en');
    expect(
      screen.getByRole('group', { name: 'Show games that' }),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: /Have DLC/ }));
    setShown(4);
    act(() => vi.advanceTimersByTime(500));
    expect(live()).toBe('4 games match these filters.');
  });
});
