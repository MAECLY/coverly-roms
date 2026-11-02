import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { Button } from '@/components/ui/button';
import { I18nProvider } from '@/i18n/I18nProvider';
import { parseNotices } from '@/lib/notices';
import { stubLayout } from '@/test/layout';

import { block } from '../../../scripts/licenses/notices.mjs';

import { NoticesDialog } from './NoticesDialog';

const RUST = [
  'lock-sha256: abc',
  '',
  block(['serde 1.0.1', 'serde_json 1.0.2'], 'MIT OR Apache-2.0', 'MIT text'),
  block(['ring 0.17.14'], 'ISC', 'ISC text, not serde'),
  block(['zip 9.0.0'], 'MIT', 'serde again and serde'),
].join('\n');
const JS = [
  'lock-sha256: abc',
  '',
  block(['react 19.3.0'], 'MIT', 'React'),
].join('\n');

vi.mock('@/lib/notices', async (importOriginal) => {
  const real = await importOriginal<typeof import('@/lib/notices')>();
  return {
    ...real,
    loadNotices: (kind: 'rust' | 'js') =>
      Promise.resolve(real.parseNotices(kind === 'rust' ? RUST : JS)),
  };
});

function renderDialog(locale: 'es' | 'en' = 'es') {
  render(
    <I18nProvider locale={locale}>
      <NoticesDialog>
        <Button>Ver licencias de terceros</Button>
      </NoticesDialog>
    </I18nProvider>,
  );
  return screen.getByRole('button', { name: 'Ver licencias de terceros' });
}

async function open(locale: 'es' | 'en' = 'es') {
  const opener = renderDialog(locale);
  await userEvent.click(opener);
  const dialog = await screen.findByRole('dialog');
  // Both files loaded: the tabs carry their counts.
  await within(dialog).findByRole('tab', { name: /\(4\)/ });
  return { opener, dialog };
}

const status = (dialog: HTMLElement) =>
  within(dialog).getByTestId('notices-status');

describe('NoticesDialog', () => {
  let restore: () => void;
  beforeEach(() => {
    restore = stubLayout(960, 640);
  });
  afterEach(() => restore());

  it('is named, starts on the find field and shows both tabs with counts', async () => {
    const { dialog } = await open();
    expect(dialog).toHaveAccessibleName('Licencias de terceros');
    expect(
      within(dialog).getByRole('tab', { name: 'Rust (4)' }),
    ).toHaveAttribute('aria-selected', 'true');
    expect(
      within(dialog).getByRole('tab', { name: 'JavaScript (1)' }),
    ).toBeVisible();
    // The counts are numbers: mono.
    expect(
      within(dialog).getByText('4', { selector: '.mono bdi' }),
    ).toBeInTheDocument();
    expect(
      within(dialog).getByRole('searchbox', { name: 'Buscar en el texto' }),
    ).toHaveFocus();
    expect(within(dialog).getByText('ring 0.17.14')).toBeVisible();
    expect(parseNotices(RUST).packageCount).toBe(4);
  });

  it('switches to the JavaScript tab', async () => {
    const { dialog } = await open();
    await userEvent.click(
      within(dialog).getByRole('tab', { name: 'JavaScript (1)' }),
    );
    expect(await within(dialog).findByText('react 19.3.0')).toBeVisible();
    expect(within(dialog).queryByText('ring 0.17.14')).toBeNull();
  });

  it('highlights the matches, announces the count and jumps with Enter / Shift+Enter', async () => {
    const { dialog } = await open();
    const find = within(dialog).getByRole('searchbox');
    await userEvent.type(find, 'serde');
    await waitFor(() => expect(status(dialog)).toHaveTextContent('1 de 5'));
    expect(status(dialog)).toHaveAttribute('aria-live', 'polite');
    const marks = () => Array.from(dialog.querySelectorAll('mark'));
    expect(marks()).toHaveLength(5);
    const current = () => dialog.querySelector('mark[data-current]');
    expect(current()?.closest('[data-block]')).toHaveAttribute(
      'data-block',
      '0',
    );
    await userEvent.keyboard('{Enter}{Enter}');
    expect(status(dialog)).toHaveTextContent('3 de 5');
    expect(current()?.closest('[data-block]')).toHaveAttribute(
      'data-block',
      '1',
    );
    await userEvent.keyboard('{Enter}');
    expect(current()?.closest('[data-block]')).toHaveAttribute(
      'data-block',
      '2',
    );
    await userEvent.keyboard('{Shift>}{Enter}{/Shift}');
    expect(status(dialog)).toHaveTextContent('3 de 5');
    // Past the last match it wraps to the first.
    await userEvent.keyboard('{Enter}{Enter}{Enter}');
    expect(status(dialog)).toHaveTextContent('1 de 5');
  });

  it('lets the keyboard reach and scroll the licence text', async () => {
    const { dialog } = await open();
    const text = within(dialog).getByRole('region', { name: 'Rust (4)' });
    expect(text).toHaveClass('notices__scroll');
    expect(text).toHaveAttribute('tabindex', '0');
    // Reading order: find, previous, next, then the text.
    await userEvent.type(within(dialog).getByRole('searchbox'), 'serde');
    await waitFor(() => expect(status(dialog)).toHaveTextContent('1 de 5'));
    await userEvent.tab();
    await userEvent.tab();
    await userEvent.tab();
    expect(text).toHaveFocus();
  });

  it('says when nothing matches', async () => {
    const { dialog } = await open();
    await userEvent.type(within(dialog).getByRole('searchbox'), 'zzzz');
    await waitFor(() =>
      expect(status(dialog)).toHaveTextContent('Sin coincidencias'),
    );
    expect(dialog.querySelector('mark')).toBeNull();
  });

  it('closes on Escape and returns focus to the button', async () => {
    const { opener } = await open();
    await userEvent.keyboard('{Escape}');
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    expect(opener).toHaveFocus();
  });

  it('speaks English', async () => {
    const { dialog } = await open('en');
    expect(dialog).toHaveAccessibleName('Third-party licences');
    expect(
      within(dialog).getByRole('searchbox', { name: 'Find in text' }),
    ).toBeVisible();
    await userEvent.type(within(dialog).getByRole('searchbox'), 'isc');
    await waitFor(() => expect(status(dialog)).toHaveTextContent('1 of 2'));
  });
});
