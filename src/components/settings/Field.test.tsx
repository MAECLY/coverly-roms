import { act, fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';
import { resetNavStore, useNavStore } from '@/stores/navStore';
import { resetSettingsStore, useSettingsStore } from '@/stores/settingsStore';

import { AdvancedFold, MonoIn, SwitchSetting } from './Field';
import { ListSetting } from './ListSetting';

const lines = (text: string) =>
  text
    .split('\n')
    .map((s) => s.trim())
    .filter(Boolean);

function Fold() {
  return (
    <AdvancedFold id="t" count={2} errorPaths={['library.ignored_dirs']}>
      <ListSetting
        path="library.ignored_dirs"
        label="Ignored folders"
        hint="One per line."
        value={[]}
        split={lines}
        join={(l) => l.join('\n')}
        toPatch={(ignored_dirs) => ({ library: { ignored_dirs } })}
      />
    </AdvancedFold>
  );
}

const renderIn = (ui: React.ReactNode) =>
  render(<I18nProvider locale="en">{ui}</I18nProvider>);

describe('settings primitives', () => {
  beforeEach(() => {
    resetSettingsStore();
    resetNavStore();
    useSettingsStore.setState({ errors: {} });
  });

  it('more_info_toggles', async () => {
    const user = userEvent.setup();
    renderIn(
      <SwitchSetting
        id="setting-x"
        name="setting-x"
        label="Thing"
        hint="Short."
        more="Long."
        checked={false}
      />,
    );
    const more = screen.getByRole('button', { name: 'More information' });
    expect(more).toHaveAttribute('aria-expanded', 'false');
    const panel = document.getElementById(more.getAttribute('aria-controls')!);
    expect(panel).not.toBeNull();
    expect(panel).toHaveAttribute('hidden');
    await user.click(more);
    expect(more).toHaveAttribute('aria-expanded', 'true');
    expect(more).toHaveTextContent('Less information');
    expect(screen.getByText('Long.')).toBeVisible();
    expect(
      screen.getByRole('switch', { name: 'Thing' }),
    ).toHaveAccessibleDescription('Short.');
    expect(
      screen.getByRole('switch').closest('[data-setting-row]'),
    ).not.toBeNull();
  });

  it('advanced_fold_is_closed_and_counts', () => {
    renderIn(<Fold />);
    const trigger = screen.getByRole('button', {
      name: 'Advanced options (2)',
    });
    expect(trigger).toHaveAttribute('aria-expanded', 'false');
    expect(screen.queryByRole('textbox')).toBeNull();
    const area = document.getElementById('setting-library-ignored-dirs');
    expect(area).not.toBeNull();
    expect(area!.closest('[hidden]')).not.toBeNull();
    expect(area!.closest('[data-setting-row]')).not.toBeNull();
    expect(screen.getByText('(2)')).toHaveClass('font-mono');
    expect(trigger.closest('[data-advanced-fold="t"]')).not.toBeNull();
  });

  it('advanced_fold_keeps_a_draft_when_closed', async () => {
    const user = userEvent.setup();
    renderIn(<Fold />);
    const trigger = screen.getByRole('button', {
      name: 'Advanced options (2)',
    });
    await user.click(trigger);
    expect(useNavStore.getState().openFolds).toContain('t');
    await user.type(screen.getByRole('textbox'), 'bios');
    // fireEvent does not move focus, so the textbox is not blurred (no commit).
    fireEvent.click(trigger);
    expect(trigger).toHaveAttribute('aria-expanded', 'false');
    expect(screen.queryByRole('textbox')).toBeNull();
    fireEvent.click(trigger);
    expect(screen.getByRole('textbox')).toHaveValue('bios');
  });

  it('advanced_fold_opens_on_error_and_flags_when_closed', async () => {
    const user = userEvent.setup();
    renderIn(<Fold />);
    expect(document.querySelector('[data-fold-error]')).toBeNull();
    act(() =>
      useSettingsStore.setState({
        errors: {
          'library.ignored_dirs': {
            field: 'library.ignored_dirs',
            code: 'invalid',
            min: null,
            max: null,
          },
        },
      }),
    );
    expect(useNavStore.getState().openFolds).toContain('t');
    const trigger = screen.getByRole('button', {
      name: 'Advanced options (2)',
    });
    expect(trigger).toHaveAttribute('aria-expanded', 'true');
    await user.click(trigger);
    expect(
      screen.getByRole('button', {
        name: 'Advanced options (2), with errors',
      }),
    ).toHaveAttribute('aria-expanded', 'false');
    expect(document.querySelector('[data-fold-error]')).not.toBeNull();
  });

  it('advanced_fold_opens_when_mounted_with_an_error_inside', () => {
    useSettingsStore.setState({
      errors: {
        'library.ignored_dirs.0': {
          field: 'library.ignored_dirs.0',
          code: 'invalid',
          min: null,
          max: null,
        },
      },
    });
    renderIn(<Fold />);
    expect(useNavStore.getState().openFolds).toContain('t');
    expect(screen.getByRole('textbox')).toBeVisible();
  });

  it('advanced_fold_ignores_errors_outside_its_paths', () => {
    useSettingsStore.setState({
      errors: {
        'library.ignored_dirs_extra': {
          field: 'library.ignored_dirs_extra',
          code: 'invalid',
          min: null,
          max: null,
        },
      },
    });
    renderIn(<Fold />);
    expect(useNavStore.getState().openFolds).not.toContain('t');
    expect(
      screen.getByRole('button', { name: 'Advanced options (2)' }),
    ).toHaveAttribute('aria-expanded', 'false');
    expect(document.querySelector('[data-fold-error]')).toBeNull();
  });

  it('more_info_on_a_field_keeps_the_short_hint_as_description', async () => {
    const user = userEvent.setup();
    renderIn(
      <ListSetting
        path="library.ignored_dirs"
        label="Ignored folders"
        hint="One per line."
        more="Folders named here are skipped at any depth."
        value={[]}
        split={lines}
        join={(l) => l.join('\n')}
        toPatch={(ignored_dirs) => ({ library: { ignored_dirs } })}
      />,
    );
    const box = screen.getByRole('textbox', { name: 'Ignored folders' });
    expect(box).toHaveAccessibleDescription('One per line.');
    const more = screen.getByRole('button', { name: 'More information' });
    expect(more).toHaveAttribute(
      'aria-controls',
      'setting-library-ignored-dirs-more',
    );
    expect(
      screen.getByText('Folders named here are skipped at any depth.'),
    ).not.toBeVisible();
    await user.click(more);
    expect(
      screen.getByText('Folders named here are skipped at any depth.'),
    ).toBeVisible();
    expect(box).toHaveAccessibleDescription('One per line.');
  });

  it('mono_in_puts_the_value_in_the_data_face', () => {
    renderIn(<MonoIn text={'Found: \u0000 here'} value="7z 24.08" />);
    expect(screen.getByText('7z 24.08')).toHaveClass('font-mono');
  });
});
