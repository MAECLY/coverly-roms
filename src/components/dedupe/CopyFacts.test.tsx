import { act, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CopyFacts, RuleChip } from '@/components/dedupe/CopyFacts';
import { TooltipProvider } from '@/components/ui/tooltip';
import { I18nProvider } from '@/i18n/I18nProvider';
import type { Locale } from '@/i18n/core';
import type { DupGroupView, MemberView } from '@/ipc/dedupeTypes';
import { dedupeFixture } from '@/ipc/mock/dedupeFixture';
import { newestMtime } from '@/lib/dedupe';
import { formatDate } from '@/lib/format';

const MTIME_MS = 1_700_000_000_000;

function fixtureGroup(title: string): DupGroupView {
  const g = dedupeFixture().groups.find((x) => x.title === title);
  if (!g) throw new Error(`no fixture group ${title}`);
  return g;
}

function copy(title: string, id: number): MemberView {
  const m = fixtureGroup(title).members.find((x) => x.id === id);
  if (!m) throw new Error(`no member ${id} in ${title}`);
  return m;
}

function renderIn(ui: React.ReactNode, locale: Locale = 'es') {
  return render(
    <I18nProvider locale={locale}>
      <TooltipProvider delayDuration={0}>{ui}</TooltipProvider>
    </I18nProvider>,
  );
}

afterEach(() => {
  vi.useRealTimers();
});

describe('CopyFacts', () => {
  it('shows the Tetris keeper tags, format, size and date', () => {
    const { container } = renderIn(<CopyFacts member={copy('Tetris', 0)} />);
    expect(screen.getByText('WLD')).toBeInTheDocument();
    expect(screen.getByText('Rev 1')).toBeInTheDocument();
    expect(screen.getByText('GB')).toBeInTheDocument();
    expect(screen.getByText('32 KB')).toBeInTheDocument();
    // The CI machine's time zone decides the day; the test does not.
    const day = formatDate(newestMtime(copy('Tetris', 0).unit) / 1e9, 'es');
    expect(day).toMatch(/nov 2023/);
    const date = screen.getByText(day);
    expect(date.closest('[title]')).toHaveAttribute(
      'title',
      expect.stringMatching(new RegExp(`^Modificado el ${day}, \\d`)),
    );
    // Region names are read, not spelled out as letters.
    expect(container.querySelector('.sr-only')).toHaveTextContent('Mundo');
  });

  it('names the Japan copy region for screen readers', () => {
    renderIn(<CopyFacts member={copy('Tetris', 1)} />);
    const chip = screen.getByText('JPN').closest('.dup-tag')!;
    expect(chip).toHaveTextContent('Japón');
    expect(within(chip as HTMLElement).getByText('Japón')).toHaveClass(
      'sr-only',
    );
  });

  it('puts the tag sources in one focusable region', async () => {
    renderIn(<CopyFacts member={copy('Tetris', 1)} />);
    const tags = screen.getByRole('group', {
      name: 'Del nombre del archivo',
    });
    expect(tags).toHaveAttribute('tabindex', '0');
    expect(tags.querySelectorAll('[tabindex]')).toHaveLength(0);
    act(() => tags.focus());
    expect(await screen.findByRole('tooltip')).toHaveTextContent(
      'Del nombre del archivo',
    );
  });

  it('says when the Crash Bash keeper was last saved and how often it was played', () => {
    vi.useFakeTimers({ toFake: ['Date'] });
    vi.setSystemTime(MTIME_MS);
    renderIn(<CopyFacts member={copy('Crash Bash', 0)} />);
    expect(screen.getByText('Partida: hace 3 días')).toBeInTheDocument();
    expect(screen.getByText('Jugada 4 veces')).toBeInTheDocument();
    expect(screen.getByText('PBP')).toBeInTheDocument();
  });

  it('counts several saves and says played once', () => {
    vi.useFakeTimers({ toFake: ['Date'] });
    vi.setSystemTime(MTIME_MS);
    const m = copy('Crash Bash', 0);
    renderIn(
      <CopyFacts
        member={{
          ...m,
          facts: {
            saves: { count: 2, last_ns: MTIME_MS * 1e6 - 3_600_000_000_000 },
            played: { count: 1, last: null },
          },
        }}
      />,
      'en',
    );
    expect(screen.getByText('2 saves, latest 1 hour ago')).toBeInTheDocument();
    expect(screen.getByText('Played once')).toBeInTheDocument();
  });

  it('marks a bad dump in the danger tone', () => {
    renderIn(<CopyFacts member={copy('Ecco the Dolphin', 1)} />);
    const chip = screen.getByText('Volcado malo [b]').closest('.dup-tag');
    expect(chip).toHaveAttribute('data-tone', 'danger');
  });

  it('renders only format, size and date for a copy without facts', () => {
    const m = copy('Crash Bash', 2);
    expect(m.facts).toBeUndefined();
    const { container } = renderIn(<CopyFacts member={m} />);
    expect(screen.queryByRole('group')).toBeNull();
    expect(container.querySelectorAll('.dup-tag')).toHaveLength(0);
    const items = container.querySelectorAll('.dup-facts__item');
    expect(items).toHaveLength(3);
    expect(screen.queryByText(/Partida|Jugada|Sin partida/)).toBeNull();
  });

  it('says «Sin partida» only when asked', () => {
    const m = copy('Crash Bash', 2);
    const { unmount } = renderIn(<CopyFacts member={m} />);
    expect(screen.queryByText('Sin partida')).toBeNull();
    unmount();
    renderIn(<CopyFacts member={m} showNoSave />);
    expect(screen.getByText('Sin partida')).toBeInTheDocument();
  });
});

describe('RuleChip', () => {
  it('names the rule and the runner-up in its tooltip', async () => {
    renderIn(
      <RuleChip
        reason={{ kind: 'region', kept: 'world', other: 'japan' }}
        userPicked={false}
        runnerUp={{
          name: 'Tetris (Japan).gb',
          reason: { kind: 'region', kept: 'world', other: 'japan' },
        }}
      />,
    );
    const chip = screen.getByText('Región');
    expect(chip).not.toHaveAttribute('data-user');
    act(() => chip.closest<HTMLElement>('[tabindex]')!.focus());
    expect(await screen.findByRole('tooltip')).toHaveTextContent(
      'Si no, ganaría Tetris (Japan).gb por región',
    );
  });

  it('reads «Tu elección» in manila when you picked, with no tab stop', () => {
    const { container } = renderIn(
      <RuleChip
        reason={{ kind: 'region', kept: 'world', other: 'japan' }}
        userPicked
        runnerUp={null}
      />,
    );
    expect(
      screen.getByText('Tu elección').closest('.dup-rule'),
    ).toHaveAttribute('data-user', '');
    expect(container.querySelector('[tabindex]')).toBeNull();
  });
});
