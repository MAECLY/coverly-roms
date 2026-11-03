import { describe, expect, it } from 'vitest';

import { translate, type Locale } from '@/i18n/core';

import { keychainMessageKey } from './keychainMessage';

/** True when the key resolves in that locale's catalogue (not to itself). */
const inCatalog = (locale: Locale) => (key: string) =>
  translate(locale, key) !== key;

describe('keychainMessageKey', () => {
  for (const locale of ['es', 'en'] as const) {
    const has = inCatalog(locale);

    it(`picks the macOS copy for a locked keychain (${locale})`, () => {
      expect(keychainMessageKey('locked', 'macos', has)).toBe(
        'errors.keychainOs.locked.macos',
      );
    });

    it(`falls back when an OS has no copy for that kind (${locale})`, () => {
      // Windows has no «locked» state (D16).
      expect(keychainMessageKey('locked', 'windows', has)).toBe(
        'errors.keychain-locked',
      );
    });

    it(`picks the Linux copy for a denied request (${locale})`, () => {
      expect(keychainMessageKey('denied', 'linux', has)).toBe(
        'errors.keychainOs.denied.linux',
      );
    });

    it(`falls back on an unknown platform (${locale})`, () => {
      expect(keychainMessageKey('unavailable', 'freebsd', has)).toBe(
        'errors.keychain-unavailable',
      );
    });

    it(`always uses the generic copy for failed (${locale})`, () => {
      for (const os of ['macos', 'windows', 'linux', 'freebsd'])
        expect(keychainMessageKey('failed', os, has)).toBe(
          'errors.keychain-failed',
        );
    });
  }

  it('never builds a key from a platform with a dot in it', () => {
    expect(keychainMessageKey('locked', 'macos.x', () => true)).toBe(
      'errors.keychain-locked',
    );
  });

  it('has the per-OS copy the spec lists, in both locales', () => {
    const wanted = [
      'locked.macos',
      'denied.macos',
      'unavailable.macos',
      'denied.windows',
      'unavailable.windows',
      'locked.linux',
      'denied.linux',
      'unavailable.linux',
    ];
    for (const locale of ['es', 'en'] as const)
      for (const k of wanted)
        expect(
          inCatalog(locale)(`errors.keychainOs.${k}`),
          `${locale} ${k}`,
        ).toBe(true);
  });
});
