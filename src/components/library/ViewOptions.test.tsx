import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';
import { DEFAULT_SORT } from '@/lib/sort';
import { DEFAULT_PREFS, VIEW_IDS, type ViewId } from '@/lib/viewPrefs';
import { useViewStore } from '@/stores/viewStore';

import { ViewOptions } from './ViewOptions';

type Control =
  'size' | 'density' | 'art' | 'group' | 'sort' | 'columns' | 'inspector';

const EXPECTED: Record<ViewId, Control[]> = {
  covers: ['size', 'art', 'group', 'sort', 'inspector'],
  details: ['density', 'group', 'columns', 'inspector'],
  columns: ['sort'],
  gallery: ['group', 'sort'],
  mounts: ['group', 'sort', 'inspector'],
  shelves: ['size', 'art', 'group', 'sort', 'inspector'],
};

async function open(view: ViewId) {
  useViewStore.getState().setPrefs({ ...DEFAULT_PREFS, view });
  const user = userEvent.setup();
  render(
    <I18nProvider locale="en">
      <ViewOptions />
    </I18nProvider>,
  );
  await user.click(screen.getByRole('button', { name: 'View options' }));
  const dialog = await screen.findByRole('dialog', { name: 'View options' });
  return { user, dialog };
}

function present(dialog: HTMLElement): Control[] {
  const q = within(dialog);
  const found: Control[] = [];
  if (q.queryByRole('slider', { name: 'Size' })) found.push('size');
  if (q.queryByRole('radiogroup', { name: 'Row density' }))
    found.push('density');
  if (q.queryByRole('radiogroup', { name: 'Art shown' })) found.push('art');
  if (q.queryByRole('combobox', { name: 'Group by' })) found.push('group');
  if (q.queryByRole('combobox', { name: 'Sort by' })) found.push('sort');
  if (q.queryByRole('group', { name: 'Visible columns' }))
    found.push('columns');
  if (q.queryByRole('switch', { name: 'Details pane' }))
    found.push('inspector');
  return found;
}

describe('ViewOptions', () => {
  beforeEach(() => useViewStore.getState().setSort(DEFAULT_SORT));
  afterEach(() => useViewStore.getState().setPrefs(DEFAULT_PREFS));

  it.each(VIEW_IDS)('shows only the controls that apply to %s', async (v) => {
    const { dialog } = await open(v);
    expect(present(dialog)).toEqual(EXPECTED[v]);
  });

  it('moves the size one stop and names the stop', async () => {
    const { user, dialog } = await open('covers');
    const slider = within(dialog).getByRole('slider', { name: 'Size' });
    expect(slider).toHaveAttribute('aria-valuetext', 'Medium');
    slider.focus();
    await user.keyboard('{ArrowRight}');
    expect(useViewStore.getState().prefs.coverSize).toBe('l');
    expect(slider).toHaveAttribute('aria-valuetext', 'Large');
  });

  it('sets the row density in Details', async () => {
    const { user, dialog } = await open('details');
    await user.click(within(dialog).getByRole('radio', { name: 'Compact' }));
    expect(useViewStore.getState().prefs.density).toBe('compact');
  });

  it('sets the art shown', async () => {
    const { user, dialog } = await open('covers');
    await user.click(
      within(dialog).getByRole('radio', { name: 'Title screen' }),
    );
    expect(useViewStore.getState().prefs.art).toBe('title');
  });

  it('groups by the chosen key', async () => {
    const { user, dialog } = await open('covers');
    await user.selectOptions(
      within(dialog).getByRole('combobox', { name: 'Group by' }),
      'folder',
    );
    expect(useViewStore.getState().prefs.groupBy).toBe('folder');
  });

  it('shows no grouping as System on Shelves, with no None choice', async () => {
    const { dialog } = await open('shelves');
    const select = within(dialog).getByRole('combobox', { name: 'Group by' });
    expect(select).toHaveValue('system');
    expect(
      within(select)
        .getAllByRole('option')
        .map((o) => (o as HTMLOptionElement).value),
    ).not.toContain('none');
  });

  it('puts missing art first on Shelves', async () => {
    const { user, dialog } = await open('shelves');
    await user.click(
      within(dialog).getByRole('button', { name: 'Missing art first' }),
    );
    expect(useViewStore.getState().prefs.groupBy).toBe('art');
  });

  it('sorts by a key and flips the direction', async () => {
    const { user, dialog } = await open('covers');
    await user.selectOptions(
      within(dialog).getByRole('combobox', { name: 'Sort by' }),
      'size',
    );
    expect(useViewStore.getState().sort).toEqual({ key: 'size', dir: 'asc' });
    const dir = within(dialog).getByRole('button', { name: 'Descending' });
    expect(dir).toHaveAttribute('aria-pressed', 'false');
    await user.click(dir);
    expect(useViewStore.getState().sort).toEqual({ key: 'size', dir: 'desc' });
    expect(dir).toHaveAttribute('aria-pressed', 'true');
  });

  it('shows, hides and reorders Details columns; the name stays', async () => {
    const { user, dialog } = await open('details');
    const list = within(dialog).getByRole('group', { name: 'Visible columns' });
    expect(within(list).getByRole('checkbox', { name: 'Name' })).toBeDisabled();
    await user.click(within(list).getByRole('checkbox', { name: 'Thumbnail' }));
    expect(useViewStore.getState().prefs.columns[0]?.id).toBe('thumb');
    await user.click(within(list).getByRole('checkbox', { name: 'Size' }));
    const ids = () => useViewStore.getState().prefs.columns.map((c) => c.id);
    expect(ids()).not.toContain('size');
    await user.click(
      within(list).getByRole('button', { name: 'Move System up' }),
    );
    expect(ids().slice(0, 3)).toEqual(['thumb', 'system', 'name']);
    expect(
      within(list).getByRole('button', { name: 'Move Thumbnail up' }),
    ).toBeDisabled();
  });

  it('docks the inspector', async () => {
    const { user, dialog } = await open('mounts');
    const sw = within(dialog).getByRole('switch', { name: 'Details pane' });
    await user.click(sw);
    expect(useViewStore.getState().prefs.inspector).toBe(true);
    expect(sw).toBeChecked();
  });
});
