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
    // One icon per candidate system, in the bucket's order, kept out of
    // the group's name.
    expect(
      [...bucket.querySelectorAll('[data-system-icon]')].map((el) =>
        el.getAttribute('data-system-icon'),
      ),
    ).toEqual(['gb', 'gbc']);
    // The backend's candidates are hints, not a decision.
    expect(bucket).toHaveAccessibleDescription(
      'Most likely: Game Boy, then Game Boy Color',
    );
    const select = within(bucket).getByRole('combobox', {
      name: 'Game Boy / Game Boy Color: 2',
    });
    expect(select).toHaveValue('');
    expect(
      within(select)
        .getAllByRole('option')
        .map((o) => o.textContent),
    ).toEqual([
      'Choose a system…',
      'Game Boy',
      'Game Boy Color',
      'Leave in place',
    ]);
    // Nothing is decided until a system is chosen.
    const apply = within(bucket).getByRole('button', {
      name: 'Apply to 2 files',
    });
    expect(apply).toBeDisabled();
    await userEvent.selectOptions(select, 'gbc');
    expect(onDecide).not.toHaveBeenCalled();
    expect(apply).toBeEnabled();
    await userEvent.click(apply);
    expect(onDecide).toHaveBeenCalledWith([3, 4], { system: 'gbc' });
  });

  it('names a single file in the apply button and can leave it in place', async () => {
    const onDecide = renderBuckets();
    const bucket = screen.getByRole('group', {
      name: 'Mega Drive / Master System: 1',
    });
    const apply = within(bucket).getByRole('button', {
      name: 'Apply to 1 file',
    });
    expect(apply).toBeDisabled();
    await userEvent.selectOptions(
      within(bucket).getByRole('combobox', {
        name: 'Mega Drive / Master System: 1',
      }),
      'leave',
    );
    await userEvent.click(apply);
    expect(onDecide).toHaveBeenCalledWith([7], 'leave');
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

  it('keeps a flat candidate list, ungrouped, with leave last', async () => {
    renderBuckets();
    await openAll();
    const one = screen.getByRole('combobox', {
      name: 'System for Columns.bin',
    });
    expect(one.querySelector('optgroup')).toBeNull();
    const values = [...one.querySelectorAll('option')].map((o) => o.value);
    expect(values.slice(-3)).toEqual(['megadrive', 'mastersystem', 'leave']);
  });

  it('wraps the picker under a long title instead of squeezing it', () => {
    renderBuckets();
    const bucket = screen.getByRole('group', {
      name: 'Game Boy / Game Boy Color: 2',
    });
    // Eight candidates make a long title: at 390 px the text keeps a
    // readable width and the select drops to the next line (flex-wrap).
    const text = bucket.querySelector('p[id]')!.parentElement!;
    expect(text).toHaveClass('min-w-0', 'flex-1', 'basis-64');
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
