import { act, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';

import { setNoEmulatorIds } from '@/hooks/useNoEmulator';
import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend } from '@/ipc/backend';
import { createMockBackend } from '@/ipc/mock/mockBackend';
import type { Section } from '@/lib/grouping';

import { SectionHeaderRow } from './SectionHeaderRow';

const shelf = (systemId: string, label: string, count = 3): Section => ({
  key: `system:${systemId}`,
  label,
  systemId,
  start: 0,
  count,
});

function renderRow(locale: 'es' | 'en', section: Section) {
  return render(
    <I18nProvider locale={locale}>
      <div role="grid">
        <SectionHeaderRow
          section={section}
          colSpan={4}
          rowIndex={0}
          style={{}}
        />
      </div>
    </I18nProvider>,
  );
}

afterEach(() => {
  setNoEmulatorIds(null);
  setBackend(null);
});

describe('SectionHeaderRow (systems batch 3 §9.1)', () => {
  it('says «Sin emulador» after the name, inside the header name', () => {
    setNoEmulatorIds(['xboxseries']);
    renderRow('es', shelf('xboxseries', 'Microsoft Xbox Series X|S'));
    const header = screen.getByRole('rowheader', {
      name: 'Microsoft Xbox Series X|S Sin emulador, 3 juegos',
    });
    const label = screen.getByText('Sin emulador');
    expect(header).toContainElement(label);
    expect(label).toHaveClass('section-row__note');
    // After the name, the dot between them not read.
    const dot = label.previousElementSibling!;
    expect(dot).toHaveTextContent('·');
    expect(dot).toHaveAttribute('aria-hidden', 'true');
    expect(dot.parentElement).toHaveTextContent(
      'Microsoft Xbox Series X|S · Sin emulador',
    );
  });

  it('keeps the name in sight when a narrow shelf has no room for both', () => {
    setNoEmulatorIds(['xboxseries']);
    const { container } = renderRow(
      'es',
      shelf('xboxseries', 'Microsoft Xbox Series X|S'),
    );
    // The note rides in the name's own ellipsis run: a narrow shelf cuts
    // «· Sin emulador» first, never the system name down to nothing.
    const name = container.querySelector('.section-row__name')!;
    expect(name).toContainElement(screen.getByText('Sin emulador'));
    expect(name).toHaveTextContent(
      /^Microsoft Xbox Series X\|S · Sin emulador$/,
    );
  });

  it('says nothing on a shelf whose system has an emulator', () => {
    setNoEmulatorIds(['xboxseries']);
    renderRow('en', shelf('ps4', 'Sony PlayStation 4', 2));
    expect(
      screen.getByRole('rowheader', { name: 'Sony PlayStation 4, 2 games' }),
    ).toBeInTheDocument();
    expect(screen.queryByText('No emulator')).toBeNull();
  });

  it('reads the catalogue from the backend once it answers', async () => {
    setBackend(createMockBackend({ delayMs: 0 }));
    renderRow('en', shelf('xboxseries', 'Microsoft Xbox Series X|S', 2));
    expect(
      await screen.findByRole('rowheader', {
        name: 'Microsoft Xbox Series X|S No emulator, 2 games',
      }),
    ).toBeInTheDocument();
    await act(async () => {});
  });
});
