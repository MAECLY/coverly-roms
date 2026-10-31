import { render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ConflictGroup } from '@/components/dedupe/ConflictGroup';
import { DupGroup } from '@/components/dedupe/DupGroup';
import { GroupCover, OwnArtThumb } from '@/components/dedupe/GroupCover';
import { TooltipProvider } from '@/components/ui/tooltip';
import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend } from '@/ipc/backend';
import type { DupGroupView } from '@/ipc/dedupeTypes';
import { BackendError } from '@/ipc/errors';
import { dedupeFixture } from '@/ipc/mock/dedupeFixture';
import { createMockBackend } from '@/ipc/mock/mockBackend';
import { clearArtCache } from '@/lib/artCache';

const PNG = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
const QUARANTINE = '/Volumes/SD/_Duplicados';

function fixtureGroup(title: string): DupGroupView {
  const g = dedupeFixture().groups.find((x) => x.title === title);
  if (!g) throw new Error(`no fixture group ${title}`);
  return g;
}

function renderIn(ui: React.ReactNode) {
  return render(
    <I18nProvider locale="es">
      <TooltipProvider delayDuration={0}>{ui}</TooltipProvider>
    </I18nProvider>,
  );
}

function backendWith(dedupeArt: (...a: unknown[]) => Promise<ArrayBuffer>) {
  const spy = vi.fn(dedupeArt);
  setBackend({ ...createMockBackend({ delayMs: 0 }), dedupeArt: spy });
  return spy;
}

const resolves = () => backendWith(async () => PNG.slice().buffer);
const missing = () =>
  backendWith(async () => {
    throw new BackendError('art-missing', 'cover');
  });

beforeEach(() => {
  let n = 0;
  vi.spyOn(URL, 'createObjectURL').mockImplementation(() => `blob:art-${n++}`);
  vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => undefined);
});

afterEach(() => {
  clearArtCache();
  setBackend(null);
  vi.restoreAllMocks();
});

describe('GroupCover', () => {
  it('shows the group cover once its bytes arrive', async () => {
    const art = resolves();
    const g = fixtureGroup('Ecco the Dolphin');
    const { container } = renderIn(<GroupCover planId={3} group={g} enabled />);
    const slot = container.querySelector('.dup-cover')!;
    expect(slot).toHaveAttribute('data-state', 'loading');
    await waitFor(() => expect(container.querySelector('img')).not.toBeNull());
    expect(slot).toHaveAttribute('data-state', 'ready');
    const img = container.querySelector('img')!;
    expect(img).toHaveAttribute('src', 'blob:art-0');
    // The game title names the group; the cover is decorative.
    expect(img).toHaveAttribute('alt', '');
    expect(art).toHaveBeenCalledWith(3, g.id, null, 'cover');
  });

  it('shows the missing slot, not an error, when there is no art', async () => {
    missing();
    const { container } = renderIn(
      <GroupCover planId={3} group={fixtureGroup('Crash Bash')} enabled />,
    );
    expect(await screen.findByText('Sin portada')).toBeInTheDocument();
    expect(container.querySelector('.dup-cover')).toHaveAttribute(
      'data-state',
      'missing',
    );
    expect(container.querySelector('img')).toBeNull();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('asks for nothing while its row is not mounted', () => {
    const art = resolves();
    renderIn(
      <GroupCover
        planId={3}
        group={fixtureGroup('Ecco the Dolphin')}
        enabled={false}
      />,
    );
    expect(art).not.toHaveBeenCalled();
  });

  it('asks again when another keeper changes whose art is the cover', async () => {
    // Rust resolves the cover keeper first (D4), under the same plan id.
    const art = resolves();
    const g = fixtureGroup('Tetris');
    const other = g.members.find((m) => m.id !== g.keeper)!;
    const { container, rerender } = renderIn(
      <GroupCover planId={3} group={g} enabled />,
    );
    await waitFor(() =>
      expect(container.querySelector('img')).toHaveAttribute(
        'src',
        'blob:art-0',
      ),
    );
    rerender(
      <I18nProvider locale="es">
        <TooltipProvider delayDuration={0}>
          <GroupCover planId={3} group={{ ...g, keeper: other.id }} enabled />
        </TooltipProvider>
      </I18nProvider>,
    );
    await waitFor(() =>
      expect(container.querySelector('img')).toHaveAttribute(
        'src',
        'blob:art-1',
      ),
    );
    expect(art).toHaveBeenCalledTimes(2);
  });

  it('names a title screen used as the cover', async () => {
    const art = resolves();
    const g = fixtureGroup('Ecco the Dolphin');
    const withTitle: DupGroupView = {
      ...g,
      members: g.members.map((m) => ({
        ...m,
        facts: {
          art: [{ media: 'title', path: 'SMS/Titles/Ecco.png', size: 99 }],
        },
      })),
    };
    renderIn(<GroupCover planId={3} group={withTitle} enabled />);
    expect(screen.getByText('Pantalla de título')).toBeInTheDocument();
    await waitFor(() =>
      expect(art).toHaveBeenCalledWith(3, g.id, null, 'title'),
    );
  });
});

describe('OwnArtThumb', () => {
  it('asks for the copy art when it differs from the cover', async () => {
    const art = resolves();
    const g = fixtureGroup('Tetris');
    renderIn(
      <OwnArtThumb
        planId={3}
        group={g}
        member={g.members[1]!}
        media="cover"
        enabled
      />,
    );
    const img = await screen.findByRole('img', {
      name: 'Esta copia tiene otra imagen',
    });
    expect(img).toHaveAttribute('src', 'blob:art-0');
    expect(art).toHaveBeenCalledWith(3, g.id, 1, 'cover');
  });

  it('renders nothing for the copy that is the cover', () => {
    const art = resolves();
    const g = fixtureGroup('Tetris');
    const { container } = renderIn(
      <OwnArtThumb
        planId={3}
        group={g}
        member={g.members[0]!}
        media="cover"
        enabled
      />,
    );
    expect(container.querySelector('.dup-own-art')).toBeNull();
    expect(art).not.toHaveBeenCalled();
  });
});

describe('DupGroup covers', () => {
  function renderGroup(group: DupGroupView, planId?: number) {
    return renderIn(
      <DupGroup group={group} quarantine={QUARANTINE} planId={planId} />,
    );
  }

  it('has a cover column with a plan, and none without (Organise)', () => {
    resolves();
    const g = fixtureGroup('Ecco the Dolphin');
    const withPlan = renderGroup(g, 3);
    expect(withPlan.container.querySelector('.dup-cover')).not.toBeNull();
    withPlan.unmount();
    const organise = renderGroup(g);
    expect(organise.container.querySelector('.dup-cover')).toBeNull();
    expect(organise.container.querySelector('.dup-own-art')).toBeNull();
  });

  it('shows the Tetris Japan box beside its copy', async () => {
    resolves();
    const { container } = renderGroup(fixtureGroup('Tetris'), 3);
    const rows = [...container.querySelectorAll('.dup-row')];
    const japan = rows.find((r) => r.textContent?.includes('Tetris (Japan)'))!;
    expect(japan.querySelector('.dup-own-art')).not.toBeNull();
    const world = rows.find((r) => r.textContent?.includes('(World)'))!;
    expect(world.querySelector('.dup-own-art')).toBeNull();
    await screen.findByRole('img', { name: 'Esta copia tiene otra imagen' });
  });

  it('shows no thumbnail on a copy without art of its own', () => {
    resolves();
    const { container } = renderGroup(fixtureGroup('Ecco the Dolphin'), 3);
    expect(container.querySelector('.dup-own-art')).toBeNull();
  });
});

describe('ConflictGroup covers', () => {
  it('shows the game cover beside a conflict, as any other group', async () => {
    const art = resolves();
    const g = fixtureGroup('Pokemon Red');
    const { container } = renderIn(<ConflictGroup group={g} planId={3} />);
    const group = container.querySelector('[data-conflict]')!;
    expect(group).toHaveAttribute('data-cover', '');
    expect(group.querySelector('.dup-cover')).not.toBeNull();
    await waitFor(() =>
      expect(art).toHaveBeenCalledWith(3, g.id, null, 'cover'),
    );
  });

  it('has no cover column without a plan', () => {
    const art = resolves();
    const { container } = renderIn(
      <ConflictGroup group={fixtureGroup('Pokemon Red')} />,
    );
    expect(container.querySelector('.dup-cover')).toBeNull();
    expect(art).not.toHaveBeenCalled();
  });
});
