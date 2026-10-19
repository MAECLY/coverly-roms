import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';
import { generateLibrary } from '@/ipc/mock/fixture';
import { useViewStore } from '@/stores/viewStore';

import { Sidebar } from './Sidebar';

const NOW = 1_760_000_000;
const view = generateLibrary({
  seed: 7,
  count: 240,
  root: '/mock/Small library',
  generation: 1,
  now: NOW,
});
const shown = view.systems.filter((s) => s.roms > 0);

describe('Sidebar', () => {
  afterEach(() => useViewStore.getState().reset());

  it('shows each system icon beside its name in the Systems tab', async () => {
    render(
      <I18nProvider locale="en">
        <Sidebar view={view} now={NOW} />
      </I18nProvider>,
    );
    await userEvent.click(screen.getByRole('tab', { name: 'Systems' }));
    const panel = screen.getByRole('tabpanel', { name: 'Systems' });
    const buttons = within(panel).getAllByRole('button');
    expect(shown.length).toBeGreaterThan(1);
    expect(buttons).toHaveLength(shown.length);
    shown.forEach((s, i) => {
      const button = buttons[i]!;
      const icon = button.querySelector(`[data-system-icon="${s.system_id}"]`);
      expect(icon).not.toBeNull();
      expect(icon).toHaveAttribute('aria-hidden', 'true');
      expect(button).toHaveAccessibleName(
        expect.stringMatching(new RegExp(`^${escape(s.name)}`)),
      );
    });
  });

  it('keeps folder and date buttons free of system icons', () => {
    const { container } = render(
      <I18nProvider locale="en">
        <Sidebar view={view} now={NOW} />
      </I18nProvider>,
    );
    expect(container.querySelector('[data-system-icon]')).toBeNull();
  });
});

function escape(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}
