import { describe, expect, it } from 'vitest';

import {
  revealCopyKey,
  revealFolderKey,
  revealLabelKey,
  revealOs,
  revealShortcut,
} from './reveal';

const MAC =
  'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko)';
const WIN =
  'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko)';
const LINUX = 'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36';

describe('reveal labels', () => {
  it('maps the user agent to an OS', () => {
    expect(revealOs(MAC)).toBe('mac');
    expect(revealOs(WIN)).toBe('win');
    expect(revealOs(LINUX)).toBe('other');
    expect(revealOs('')).toBe('other');
  });

  it('names the label, copy and folder keys per OS', () => {
    expect(revealLabelKey('mac')).toBe('drawer.revealMac');
    expect(revealLabelKey('win')).toBe('drawer.revealWin');
    expect(revealLabelKey('other')).toBe('drawer.revealOther');
    expect(revealCopyKey('mac')).toBe('dedupe.revealCopy.mac');
    expect(revealCopyKey('win')).toBe('dedupe.revealCopy.win');
    expect(revealFolderKey('other')).toBe('dedupe.revealFolder.other');
  });

  it('advertises the chord per OS', () => {
    expect(revealShortcut('mac')).toEqual({ label: '⌥⌘R', aria: 'Meta+Alt+R' });
    expect(revealShortcut('win')).toEqual({
      label: 'Shift+Alt+R',
      aria: 'Shift+Alt+R',
    });
    expect(revealShortcut('other')).toEqual({
      label: 'Shift+Alt+R',
      aria: 'Shift+Alt+R',
    });
  });
});
