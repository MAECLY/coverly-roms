import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { ReviewBuckets } from '@/components/organize/ReviewBuckets';
import { I18nProvider } from '@/i18n/I18nProvider';
import { reviewBuckets } from '@/lib/organize';
import { planOf, reviewItem } from '@/test/organizeFixtures';

const NAMES: Record<string, string> = {
  gb: 'Game Boy',
  gbc: 'Game Boy Color',
  megadrive: 'Mega Drive',
  mastersystem: 'Master System',
};
const systemName = (id: string) => NAMES[id] ?? id;

function renderBuckets() {
  const onDecide = vi.fn();
  const plan = planOf([
    reviewItem(3, 'Pokemon Pinball.zip', [
      ['gb', 0.55],
      ['gbc', 0.45],
    ]),
    reviewItem(4, 'Tetris.zip', [
      ['gbc', 0.6],
      ['gb', 0.4],
    ]),
    reviewItem(7, 'Columns.bin', [
      ['megadrive', 0.5],
      ['mastersystem', 0.3],
    ]),
  ]);
  render(
    <I18nProvider locale="en">
      <ReviewBuckets
        buckets={reviewBuckets(plan)}
        systemName={systemName}
        onDecide={onDecide}
      />
    </I18nProvider>,
  );
  return onDecide;
}

/** Opens every bucket's file list (rows render only once opened). */
async function openAll() {
  for (const toggle of screen.getAllByRole('button', { name: /^Files: / }))
    await userEvent.click(toggle);
}

describe('ReviewBuckets', () => {
  it('buckets by candidate set and applies one choice to the whole bucket', async () => {
    const onDecide = renderBuckets();
    expect(
      screen.getByRole('heading', { name: 'To review' }),
    ).toBeInTheDocument();
    expect(
      screen.getByText("Choose each file's system, or leave it where it is."),
    ).toBeInTheDocument();
    const bucket = screen.getByRole('group', {
      name: 'Game Boy / Game Boy Color: 2',
    });
    expect(
      screen.getByRole('group', { name: 'Mega Drive / Master System: 1' }),
    ).toBeInTheDocument();
    const select = within(bucket).getByRole('combobox', {
      name: 'Game Boy / Game Boy Color: 2',
    });
    expect(
      within(select)
        .getAllByRole('option')
        .map((o) => o.textContent),
    ).toEqual(['Game Boy', 'Game Boy Color', 'Leave in place']);
    await userEvent.selectOptions(select, 'gbc');
    expect(onDecide).not.toHaveBeenCalled();
    await userEvent.click(
      within(bucket).getByRole('button', { name: 'Apply to all' }),
    );
    expect(onDecide).toHaveBeenCalledWith([3, 4], { system: 'gbc' });
  });

  it('settles one file, or leaves it in place, from its own select', async () => {
    const onDecide = renderBuckets();
    expect(
      screen.queryByRole('combobox', { name: 'System for Columns.bin' }),
    ).not.toBeInTheDocument();
    await openAll();
    const one = screen.getByRole('combobox', {
      name: 'System for Columns.bin',
    });
    await userEvent.selectOptions(one, 'mastersystem');
    expect(onDecide).toHaveBeenLastCalledWith([7], { system: 'mastersystem' });
    await userEvent.selectOptions(
      screen.getByRole('combobox', { name: 'System for Tetris.zip' }),
      'leave',
    );
    expect(onDecide).toHaveBeenLastCalledWith([4], 'leave');
  });

  it('shows the evidence of each file', async () => {
    renderBuckets();
    await openAll();
    expect(
      screen.getAllByText(
        '.gb extension, shared by several systems; Game Boy is likely',
      ),
    ).not.toHaveLength(0);
  });

  it('renders a large bucket capped, and only once opened', async () => {
    const items = Array.from({ length: 450 }, (_, i) =>
      reviewItem(i, `Game ${String(i).padStart(3, '0')}.gb`, [
        ['gb', 0.55],
        ['gbc', 0.45],
      ]),
    );
    render(
      <I18nProvider locale="en">
        <ReviewBuckets
          buckets={reviewBuckets(planOf(items))}
          systemName={systemName}
          onDecide={vi.fn()}
        />
      </I18nProvider>,
    );
    const bucket = screen.getByRole('group', {
      name: 'Game Boy / Game Boy Color: 450',
    });
    const toggle = within(bucket).getByRole('button', { name: 'Files: 450' });
    expect(toggle).toHaveAttribute('aria-expanded', 'false');
    expect(within(bucket).queryAllByRole('listitem')).toHaveLength(0);
    await userEvent.click(toggle);
    expect(toggle).toHaveAttribute('aria-expanded', 'true');
    expect(within(bucket).getAllByRole('listitem')).toHaveLength(200);
    await userEvent.click(
      within(bucket).getByRole('button', { name: 'Show 250 more' }),
    );
    expect(within(bucket).getAllByRole('listitem')).toHaveLength(400);
  });
});
