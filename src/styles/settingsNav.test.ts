/// <reference types="node" />
import { readFileSync } from 'node:fs';
import path from 'node:path';

import { describe, expect, it } from 'vitest';

import { SETTINGS_SECTIONS } from '@/stores/navStore';

import en from '../../locales/en/ui.json';
import es from '../../locales/es/ui.json';

// Read from disk: Vitest's CSS handling empties a `?raw` .css import.
const css = readFileSync(path.resolve(__dirname, 'globals.css'), 'utf8');

/** Body of the `selector` rule inside the `@media (max-width: 40rem)` block. */
function narrowRule(selector: string): string {
  const media = /@media \(max-width: 40rem\) \{([\s\S]*?)\n\}/.exec(css);
  expect(media).not.toBeNull();
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const rule = new RegExp(`${escaped} \\{([^}]*)\\}`).exec(media![1]!);
  expect(rule).not.toBeNull();
  return rule![1]!;
}

describe('settings tab row on narrow windows', () => {
  const row = () => narrowRule(".settings-nav [role='tablist']");

  it('scrolls in one row inside itself', () => {
    expect(row()).toMatch(/flex-wrap: nowrap;/);
    expect(row()).toMatch(/overflow-x: auto;/);
    expect(row()).toMatch(/overscroll-behavior-x: contain;/);
  });

  it('keeps a scrollbar so a mouse without a trackpad reaches every tab', () => {
    expect(row()).not.toMatch(/scrollbar-width: none/);
  });

  it('draws the tab focus ring inside the tab so the row does not clip it', () => {
    expect(narrowRule(".settings-nav [role='tab']:focus-visible")).toMatch(
      /outline-offset: -2px;/,
    );
  });
});

describe('settings tab column on a 760 px window', () => {
  /** Body of the first top-level `selector` rule. */
  const rule = (selector: string) => {
    const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    const m = new RegExp(`\\n${escaped} \\{([^}]*)\\}`).exec(css);
    expect(m).not.toBeNull();
    return m![1]!;
  };

  it('keeps a fixed 12rem column that never shrinks', () => {
    expect(rule('.settings-nav')).toMatch(/flex: 0 0 12rem;/);
  });

  it('every one of the 14 tab names fits that column on one line', () => {
    expect(SETTINGS_SECTIONS).toHaveLength(14);
    // 12rem minus the tab padding holds about 20 characters at 13 px.
    const long = [es, en].flatMap((c) =>
      SETTINGS_SECTIONS.map((s) => c.settings.section[s]).filter(
        (name) => name.length > 20,
      ),
    );
    expect(long).toEqual([]);
  });
});

describe('settings screen scrolls like a preferences window', () => {
  /** Body of the first top-level `selector` rule. */
  const rule = (selector: string) => {
    const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    const m = new RegExp(`\\n${escaped} \\{([^}]*)\\}`).exec(css);
    expect(m).not.toBeNull();
    return m![1]!;
  };

  it('is exactly the window tall and never scrolls the page', () => {
    const screen = rule('.settings-screen');
    expect(screen).toMatch(/height: 100dvh;/);
    expect(screen).toMatch(/overflow: hidden;/);
    expect(screen).not.toMatch(/min-height: 100vh;/);
    // Positioned: an sr-only live region inside cannot stretch the page.
    expect(screen).toMatch(/position: relative;/);
  });

  it('lets the tab column scroll on its own when the window is short', () => {
    const nav = rule('.settings-nav');
    expect(nav).toMatch(/min-height: 0;/);
    expect(nav).toMatch(/overflow-y: auto;/);
    expect(nav).toMatch(/overscroll-behavior: contain;/);
  });

  it('keeps the search at the top of a scrolled tab column', () => {
    const search = rule('.settings-nav > .settings-search');
    expect(search).toMatch(/position: sticky;/);
    expect(search).toMatch(/background: var\(--cr-bg\);/);
  });

  it('drops the sticky search in the one-row narrow layout', () => {
    const search = narrowRule('.settings-nav > .settings-search');
    expect(search).toMatch(/position: relative;/);
    expect(search).toMatch(/top: auto;/);
  });
});
