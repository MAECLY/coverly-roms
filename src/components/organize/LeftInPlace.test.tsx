import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { LeftInPlace } from '@/components/organize/LeftInPlace';
import { I18nProvider } from '@/i18n/I18nProvider';
import type { OrganizeItem } from '@/ipc/types';
import { autoItem } from '@/test/organizeFixtures';

const unknown = (id: number): OrganizeItem =>
  ({
    ...autoItem(id, `File ${String(id).padStart(3, '0')}.bin`, 'gb', 'X'),
    action: 'left',
    reason: { kind: 'unknown' },
    system_id: null,
    target: null,
  }) as OrganizeItem;

describe('LeftInPlace', () => {
  it('renders at most 200 rows and a show-more button', async () => {
    render(
      <I18nProvider locale="en">
        <LeftInPlace
          items={Array.from({ length: 450 }, (_, i) => unknown(i))}
          systems={[{ id: 'gb', name: 'Game Boy' }]}
          otherFiles={0}
          deepFolders={0}
          onDecide={vi.fn()}
        />
      </I18nProvider>,
    );
    expect(screen.getAllByRole('listitem')).toHaveLength(200);
    expect(screen.getAllByRole('combobox')).toHaveLength(200);
    await userEvent.click(
      screen.getByRole('button', { name: 'Show 250 more' }),
    );
    expect(screen.getAllByRole('listitem')).toHaveLength(400);
  });
});
