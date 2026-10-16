import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';
import type { CandidateView, PickReason } from '@/ipc/types';

import { CandidateMeta } from './CandidateMeta';

const base: CandidateView = {
  index: 0,
  source: 'steamgriddb',
  name: 'Doom',
  regions: [],
  kind: 'title-exact',
  score: 95,
  confidence: 0.95,
  votes: { up: 8, down: 1 },
  origin: 'game 5248 · grid 80211',
  reason: { why: 'votes', score: 7, next: 2 },
};

function show(c: CandidateView, locale: 'en' | 'es' = 'en') {
  return render(
    <I18nProvider locale={locale}>
      <div data-testid="meta">
        <CandidateMeta c={c} position={c.index + 1} />
      </div>
    </I18nProvider>,
  );
}

describe('CandidateMeta', () => {
  it('shows the source badge, votes and the reason', () => {
    show(base);
    expect(screen.getByText('SteamGridDB')).toHaveAttribute(
      'data-source',
      'steamgriddb',
    );
    expect(screen.getByText('7 votes (▲8 ▼1)')).toBeInTheDocument();
    expect(
      screen.getByText('Why: most SteamGridDB votes (7; next 2).'),
    ).toBeInTheDocument();
  });

  it('names every reason in Spanish', () => {
    const reasons: [PickReason, string][] = [
      [{ why: 'only' }, 'la única imagen encontrada.'],
      [
        { why: 'source-order', source: 'libretro' },
        'primera fuente en tu orden (libretro-thumbnails).',
      ],
      [{ why: 'region', rom: true }, 'misma región que la ROM.'],
      [{ why: 'region', rom: false }, 'región preferida en tu orden.'],
      [
        { why: 'consensus', count: 3, next: 1 },
        'más fuentes y tipos de arte coinciden (3; siguiente 1).',
      ],
      [{ why: 'user' }, 'elegida por ti.'],
    ];
    for (const [reason, text] of reasons) {
      const { unmount } = show({ ...base, reason }, 'es');
      expect(screen.getByText(`Por qué: ${text}`)).toBeInTheDocument();
      unmount();
    }
  });

  it('shows an unknown source by its id and skips a reason it cannot name', () => {
    show({
      ...base,
      source: 'thegamesdb',
      votes: null,
      reason: { why: 'future-thing' } as unknown as PickReason,
    });
    const meta = screen.getByTestId('meta');
    expect(screen.getByText('thegamesdb')).toHaveAttribute(
      'data-source',
      'other',
    );
    expect(meta.textContent).not.toContain('reason.');
    expect(meta.textContent).not.toContain('Why:');
    expect(meta.textContent).not.toContain('votes');
  });
});
