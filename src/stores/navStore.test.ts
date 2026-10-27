import { beforeEach, describe, expect, it } from 'vitest';

import { resetNavStore, useNavStore } from './navStore';

const nav = () => useNavStore.getState();

describe('navStore settings round trip', () => {
  beforeEach(() => resetNavStore());

  it('records the screen Settings was opened from and returns there', () => {
    nav().openDuplicates('/Volumes/SD');
    nav().openSettings('library');
    expect(nav().settingsFrom).toBe('duplicates');
    nav().closeSettings();
    expect(nav().screen).toBe('duplicates');
    expect(nav().settingsFrom).toBeNull();
    expect(nav().duplicatesRoot).toBe('/Volumes/SD');
  });

  it('does not overwrite settingsFrom from inside Settings', () => {
    nav().openOrganize('/Users/demo/Downloads');
    nav().openSettings('library');
    nav().openSettings('matching');
    expect(nav().settingsFrom).toBe('organize');
    expect(nav().section).toBe('matching');
  });

  it('falls back to main when the screen has nothing to show', () => {
    nav().openDuplicates('/Volumes/SD');
    nav().openSettings();
    useNavStore.setState({ duplicatesRoot: null });
    nav().closeSettings();
    expect(nav().screen).toBe('main');
  });

  it('keeps the last tab, ignores unknown ids and sets the focus target', () => {
    nav().openSettings('art');
    nav().closeSettings();
    nav().openSettings();
    expect(nav().section).toBe('art');
    nav().setSection('nope' as never);
    expect(nav().section).toBe('art');
    nav().openSettings('library', { focus: 'setting-library-include-hidden' });
    expect(nav().focusSetting).toBe('setting-library-include-hidden');
    nav().clearFocusSetting();
    expect(nav().focusSetting).toBeNull();
  });

  it('keeps folds open for the session, not across a reset', () => {
    nav().setFold('library-advanced', true);
    nav().setFold('library-advanced', true);
    expect(nav().openFolds).toEqual(['library-advanced']);
    nav().closeSettings();
    expect(nav().openFolds).toEqual(['library-advanced']);
    nav().setFold('library-advanced', false);
    expect(nav().openFolds).toEqual([]);
    nav().setFold('x', true);
    resetNavStore();
    expect(nav().openFolds).toEqual([]);
  });
});
