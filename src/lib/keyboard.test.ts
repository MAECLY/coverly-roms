import { describe, expect, it } from 'vitest';

import { digitOf, isRevealChord, letterOf } from './keyboard';

function key(init: KeyboardEventInit): KeyboardEvent {
  return new KeyboardEvent('keydown', init);
}

describe('digitOf', () => {
  it('reads the digit row and the numpad by code, not by character', () => {
    expect(digitOf({ code: 'Digit2' })).toBe(2);
    expect(digitOf({ code: 'Digit1' })).toBe(1);
    expect(digitOf({ code: 'Numpad9' })).toBe(9);
    // AZERTY: the 2 key types «é» without Shift; its code is still Digit2.
    expect(digitOf({ code: 'Digit2' })).toBe(2);
  });

  it('ignores 0 and everything that is not a digit key', () => {
    expect(digitOf({ code: 'Digit0' })).toBeNull();
    expect(digitOf({ code: 'Numpad0' })).toBeNull();
    expect(digitOf({ code: 'KeyA' })).toBeNull();
    expect(digitOf({ code: 'NumpadEnter' })).toBeNull();
    expect(digitOf({ code: '' })).toBeNull();
  });
});

describe('letterOf', () => {
  it('takes a Latin key as typed, lower-cased', () => {
    expect(letterOf({ key: 'j', code: 'KeyJ' })).toBe('j');
    expect(letterOf({ key: 'K', code: 'KeyK' })).toBe('k');
  });

  it('falls back to the physical key on a non-Latin layout', () => {
    expect(letterOf({ key: 'о', code: 'KeyJ' })).toBe('j');
    expect(letterOf({ key: 'л', code: 'KeyK' })).toBe('k');
  });

  it('is null for keys that are not letters', () => {
    expect(letterOf({ key: 'Enter', code: 'Enter' })).toBeNull();
    expect(letterOf({ key: '2', code: 'Digit2' })).toBeNull();
  });
});

describe('isRevealChord', () => {
  it('is ⌥⌘R on macOS, matched by code (⌥R types ®)', () => {
    expect(
      isRevealChord(
        key({ code: 'KeyR', key: '®', metaKey: true, altKey: true }),
        'mac',
      ),
    ).toBe(true);
    expect(
      isRevealChord(
        key({
          code: 'KeyR',
          key: '®',
          metaKey: true,
          altKey: true,
          ctrlKey: true,
        }),
        'mac',
      ),
    ).toBe(false);
    expect(
      isRevealChord(key({ code: 'KeyR', key: 'r', metaKey: true }), 'mac'),
    ).toBe(false);
    expect(
      isRevealChord(
        key({ code: 'KeyR', key: 'R', shiftKey: true, altKey: true }),
        'mac',
      ),
    ).toBe(false);
  });

  it('is Shift+Alt+R elsewhere', () => {
    for (const os of ['win', 'other'] as const) {
      expect(
        isRevealChord(
          key({ code: 'KeyR', key: 'R', shiftKey: true, altKey: true }),
          os,
        ),
      ).toBe(true);
      expect(
        isRevealChord(
          key({
            code: 'KeyR',
            key: 'R',
            shiftKey: true,
            altKey: true,
            ctrlKey: true,
          }),
          os,
        ),
      ).toBe(false);
      expect(
        isRevealChord(
          key({ code: 'KeyR', key: '®', metaKey: true, altKey: true }),
          os,
        ),
      ).toBe(false);
    }
  });

  it('needs the R key itself', () => {
    expect(
      isRevealChord(
        key({ code: 'KeyT', key: 't', metaKey: true, altKey: true }),
        'mac',
      ),
    ).toBe(false);
  });
});
