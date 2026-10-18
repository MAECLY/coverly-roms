import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend } from '@/ipc/backend';
import { FIXTURE_NOW, generateLibrary } from '@/ipc/mock/fixture';
import { createMockBackend, MOCK_LIBRARIES } from '@/ipc/mock/mockBackend';
import type { Media } from '@/ipc/types';
import { clearArtCache } from '@/lib/artCache';

import { QuickLook } from './QuickLook';

const lib = generateLibrary({
  seed: 12,
  count: 12_000,
  root: '/mock/Big',
  generation: 1,
  now: FIXTURE_NOW,
});
const entries = lib.entries.slice(0, 5);

function Harness({
  start = entries[1]!.id,
  media = 'cover',
  onSelect = () => undefined,
}: {
  start?: number;
  media?: Media;
  onSelect?: (id: number) => void;
}) {
  const [id, setId] = useState<number | null>(start);
  return (
    <I18nProvider locale="en">
      <QuickLook
        entries={entries}
        systems={lib.systems}
        generation={1}
        media={media}
        previewId={id}
        onSelect={(next) => {
          onSelect(next);
          setId(next);
        }}
        onClose={() => setId(null)}
      />
    </I18nProvider>
  );
}

describe('QuickLook', () => {
  beforeEach(() => {
    setBackend(createMockBackend({ delayMs: 0, now: FIXTURE_NOW }));
  });
  afterEach(() => {
    clearArtCache();
    setBackend(null);
  });

  it('opens named after the entry, with its system and place', () => {
    render(<Harness />);
    const dialog = screen.getByRole('dialog', { name: entries[1]!.name });
    expect(dialog).toHaveAccessibleDescription(
      lib.systems[entries[1]!.system]!.name,
    );
    expect(dialog.querySelector('[data-system-icon]')).not.toBeNull();
    expect(dialog).toHaveTextContent('2 of 5');
  });

  it('steps with the arrow keys and the buttons', async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    render(<Harness onSelect={onSelect} />);
    await user.keyboard('{ArrowRight}');
    expect(onSelect).toHaveBeenLastCalledWith(entries[2]!.id);
    expect(
      screen.getByRole('dialog', { name: entries[2]!.name }),
    ).toBeInTheDocument();
    await user.keyboard('{ArrowLeft}{ArrowLeft}');
    expect(onSelect).toHaveBeenLastCalledWith(entries[0]!.id);
    // The first entry has no previous one.
    expect(
      screen.getByRole('button', { name: 'Previous game' }),
    ).toBeDisabled();
    await user.keyboard('{ArrowLeft}');
    expect(onSelect).toHaveBeenCalledTimes(3);
    await user.click(screen.getByRole('button', { name: 'Next game' }));
    expect(onSelect).toHaveBeenLastCalledWith(entries[1]!.id);
  });

  it('closes with Escape', async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await user.keyboard('{Escape}');
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('closes with Space', async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await user.keyboard(' ');
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('shows the art large with a name and media alt', async () => {
    const backend = createMockBackend({ delayMs: 0, now: FIXTURE_NOW });
    setBackend(backend);
    await backend.openLibrary(MOCK_LIBRARIES[0].path);
    const entry = entries.find((e) => e.art.cover === 'present')!;
    render(<Harness start={entry.id} />);
    expect(
      await screen.findByRole('img', { name: `${entry.name}, Cover` }),
    ).toBeInTheDocument();
  });

  it('says which art is missing', async () => {
    const entry = entries.find((e) => e.art.screenshot === 'missing');
    if (!entry) return;
    render(<Harness start={entry.id} media="screenshot" />);
    await waitFor(() =>
      expect(screen.getByRole('dialog')).toHaveTextContent('No screenshot'),
    );
    expect(screen.queryByRole('img', { name: /Screenshot/ })).toBeNull();
  });

  it('announces the entry stepped to, not the one it opened on', async () => {
    const user = userEvent.setup();
    render(<Harness />);
    const live = screen
      .getByRole('dialog')
      .querySelector('[aria-live="polite"]')!;
    expect(live).toHaveTextContent('');
    await user.keyboard('{ArrowRight}');
    expect(live).toHaveTextContent(`${entries[2]!.name}, 3 of 5`);
  });

  it('forgets a previewed entry that left the list', () => {
    const onClose = vi.fn();
    render(
      <I18nProvider locale="en">
        <QuickLook
          entries={entries}
          systems={lib.systems}
          generation={1}
          media="cover"
          previewId={-7}
          onSelect={() => undefined}
          onClose={onClose}
        />
      </I18nProvider>,
    );
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('renders nothing with no entry to preview', () => {
    render(<Harness start={-1} />);
    expect(screen.queryByRole('dialog')).toBeNull();
  });
});
