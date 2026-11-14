import { renderHook } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';

import { setSystemNotes, useSystemNote } from './useSystemNote';

describe('useSystemNote', () => {
  afterEach(() => setSystemNotes(null));

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
});
