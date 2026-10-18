import { render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';
import { generateLibrary } from '@/ipc/mock/fixture';
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

function escape(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}
