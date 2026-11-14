import { renderHook, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';

import { setBackend } from '@/ipc/backend';
import { createMockBackend } from '@/ipc/mock/mockBackend';

import { setSystemNotes, useSystemNote } from './useSystemNote';

describe('useSystemNote', () => {
  afterEach(() => {
    setSystemNotes(null);
    setBackend(null);
  });

  it('names the note of each system, noEmulator first', () => {
    setSystemNotes({ noEmulator: ['xboxseries'], pcOnly: ['typex'] });
    const note = (id: string | null) =>
      renderHook(() => useSystemNote(id)).result.current;
    expect(note('typex')).toBe('pcOnly');
    expect(note('xboxseries')).toBe('noEmulator');
    expect(note('snes')).toBeNull();
    expect(note(null)).toBeNull();
    setSystemNotes({ noEmulator: ['typex'], pcOnly: ['typex'] });
    expect(note('typex')).toBe('noEmulator');
  });

  it('reads pc_only and no_emulator from the catalogue', async () => {
    setBackend(createMockBackend({ delayMs: 0 }));
    const typex = renderHook(() => useSystemNote('typex'));
    await waitFor(() => expect(typex.result.current).toBe('pcOnly'));
    expect(renderHook(() => useSystemNote('xboxseries')).result.current).toBe(
      'noEmulator',
    );
    // Cave CV1000 runs where MAME runs: no note.
    expect(renderHook(() => useSystemNote('cave3rd')).result.current).toBe(
      null,
    );
  });
});
