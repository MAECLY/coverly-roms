import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';
import { useViewStore } from '@/stores/viewStore';

import { ViewSwitcher } from './ViewSwitcher';

const NAMES = [
  'Covers',
  'Details',
  'Columns',
  'Gallery',
  'Art slots',
  'Shelves',
];

function renderSwitcher() {
  return render(
    <I18nProvider locale="en">
      <ViewSwitcher />
    </I18nProvider>,
  );
}

describe('ViewSwitcher', () => {
  afterEach(() => useViewStore.getState().setView('covers'));

  it('offers the six views in order as one radiogroup', () => {
    useViewStore.getState().setView('covers');
    renderSwitcher();
    const group = screen.getByRole('radiogroup', { name: 'View' });
    const radios = within(group).getAllByRole('radio');
    expect(radios.map((r) => r.getAttribute('aria-label'))).toEqual(NAMES);
    expect(within(group).getByRole('radio', { name: 'Covers' })).toBeChecked();
  });

  it('tells assistive tech each view’s shortcut', () => {
    renderSwitcher();
    const group = screen.getByRole('radiogroup', { name: 'View' });
    within(group)
      .getAllByRole('radio')
      .forEach((radio, i) =>
        expect(radio).toHaveAttribute(
          'aria-keyshortcuts',
          `Meta+${i + 1} Control+${i + 1}`,
        ),
      );
  });

  it('switches the view when a segment is chosen', async () => {
    renderSwitcher();
    const group = screen.getByRole('radiogroup', { name: 'View' });
    await userEvent.click(
      within(group).getByRole('radio', { name: 'Art slots' }),
    );
    expect(useViewStore.getState().prefs.view).toBe('mounts');
    expect(
      within(group).getByRole('radio', { name: 'Art slots' }),
    ).toBeChecked();
    expect(
      within(group).getByRole('radio', { name: 'Covers' }),
    ).not.toBeChecked();
  });

  it('separates Finder’s four from Coverly’s two', () => {
    renderSwitcher();
    const group = screen.getByRole('radiogroup', { name: 'View' });
    const rule = group.querySelector('.view-switcher__rule');
    expect(rule).not.toBeNull();
    expect(rule).toHaveAttribute('aria-hidden', 'true');
    const gallery = within(group).getByRole('radio', { name: 'Gallery' });
    const mounts = within(group).getByRole('radio', { name: 'Art slots' });
    expect(
      gallery.compareDocumentPosition(rule!) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(
      mounts.compareDocumentPosition(rule!) & Node.DOCUMENT_POSITION_PRECEDING,
    ).toBeTruthy();
  });

  it('lists the same six views with shortcuts in the narrow menu', async () => {
    useViewStore.getState().setView('columns');
    renderSwitcher();
    // The collapsed trigger names the current view, not just "View".
    await userEvent.click(
      screen.getByRole('button', { name: 'View, Columns' }),
    );
    const menu = await screen.findByRole('menu');
    const items = within(menu).getAllByRole('menuitemradio');
    expect(items).toHaveLength(6);
    items.forEach((item, i) => {
      expect(item).toHaveTextContent(NAMES[i]!);
      expect(item).toHaveTextContent(new RegExp(`${i + 1}$`));
      expect(item).toHaveAttribute(
        'aria-keyshortcuts',
        `Meta+${i + 1} Control+${i + 1}`,
      );
    });
    expect(
      within(menu).getByRole('menuitemradio', { name: /Columns/ }),
    ).toHaveAttribute('aria-checked', 'true');
  });

  it('keeps the menu and the segments on one value', async () => {
    renderSwitcher();
    await userEvent.click(screen.getByRole('button', { name: /View/ }));
    const menu = await screen.findByRole('menu');
    await userEvent.click(
      within(menu).getByRole('menuitemradio', { name: /Shelves/ }),
    );
    expect(useViewStore.getState().prefs.view).toBe('shelves');
    const group = screen.getByRole('radiogroup', { name: 'View' });
    expect(within(group).getByRole('radio', { name: 'Shelves' })).toBeChecked();
  });
});
