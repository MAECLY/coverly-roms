import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';
import { generateLibrary } from '@/ipc/mock/fixture';
import { pcArcadeLibrary } from '@/ipc/mock/pcArcadeFixture';
import { computeFacets, EMPTY_FILTERS } from '@/lib/filters';
import { useFilterStore } from '@/stores/filterStore';

import { FilterPanel } from './FilterPanel';

const view = generateLibrary({
  seed: 7,
  count: 240,
  root: '/mock/Small library',
  generation: 1,
  now: 1_760_000_000,
});
const shown = view.systems.filter((s) => s.roms > 0);
const facets = computeFacets(view.entries, [], EMPTY_FILTERS);

describe('FilterPanel', () => {
  afterEach(() => useFilterStore.getState().reset());

  it('shows each system icon between the checkbox and its name', () => {
    render(
      <I18nProvider locale="en">
        <FilterPanel view={view} facets={facets} />
      </I18nProvider>,
    );
    expect(shown.length).toBeGreaterThan(1);
    for (const s of shown) {
      // The accessible name is still the system name and its count; the
      // decorative icon (and any abbreviation in it) adds nothing.
      const box = screen.getByRole('checkbox', {
        name: new RegExp(`^${escape(s.name)}\\s*[\\d,.]+$`),
      });
      const label = document.querySelector(`label[for="${box.id}"]`)!;
      const icon = label.querySelector(`[data-system-icon="${s.system_id}"]`);
      expect(icon).not.toBeNull();
      expect(icon).toHaveAttribute('aria-hidden', 'true');
      // The icon leads the label text, after the control.
      expect(label.firstElementChild).toBe(icon);
    }
  });

  it('keeps other groups free of system icons', () => {
    const { container } = render(
      <I18nProvider locale="en">
        <FilterPanel view={view} facets={facets} />
      </I18nProvider>,
    );
    expect(container.querySelectorAll('[data-system-icon]')).toHaveLength(
      shown.length,
    );
  });
});

describe('FilterPanel extensions', () => {
  afterEach(() => useFilterStore.getState().reset());

  it('names folder games without a suffix instead of a bare dot', () => {
    const pc = pcArcadeLibrary(1, 1_760_000_000);
    const pcFacets = computeFacets(pc.entries, [], EMPTY_FILTERS);
    for (const [locale, label] of [
      ['es', 'Sin extensión'],
      ['en', 'No extension'],
    ] as const) {
      const { unmount } = render(
        <I18nProvider locale={locale}>
          <FilterPanel view={pc} facets={pcFacets} />
        </I18nProvider>,
      );
      const box = screen.getByRole('checkbox', {
        name: new RegExp(`^${label}\\s*3$`),
      });
      expect(box).toHaveAttribute('value', '');
      expect(screen.queryByText('.', { exact: true })).toBeNull();
      expect(screen.getByText('.exe')).toBeInTheDocument();
      unmount();
    }
  });
});

describe('FilterPanel «Quitar filtros»', () => {
  afterEach(() => useFilterStore.getState().reset());

  it('also releases the content chips (D14) and keeps the search', () => {
    act(() => {
      useFilterStore.getState().toggleContent('dlc');
      useFilterStore.getState().setQuery('harbor');
    });
    render(
      <I18nProvider locale="es">
        <FilterPanel view={view} facets={facets} />
      </I18nProvider>,
    );
    fireEvent.click(screen.getByRole('button', { name: 'Quitar filtros' }));
    const { filters } = useFilterStore.getState();
    expect(filters.content).toEqual(EMPTY_FILTERS.content);
    expect(filters.query).toBe('harbor');
  });
});

function escape(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}
