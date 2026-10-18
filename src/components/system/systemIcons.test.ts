import { describe, expect, it } from 'vitest';
import toml from '../../../crates/coverly-core/src/systems/systems.toml?raw';
import { SYSTEM_ICON_SOURCES } from '../../../scripts/icons/systemIconsLib.mjs';
import {
  abbreviationFor,
  FALLBACK_ABBR,
  WORDMARK_IDS,
} from './systemFallbacks';
import { SYSTEM_ICONS } from './systemIcons.generated';

const IDS = [...toml.matchAll(/^id\s*=\s*"([^"]+)"/gm)].map((m) => m[1]!);

describe('system icon data', () => {
  it('reads the 54 systems', () => expect(IDS).toHaveLength(54));
  it('every systems.toml id resolves to an icon or an abbreviation, not both', () => {
    for (const id of IDS) {
      const has = Number(id in SYSTEM_ICONS) + Number(id in FALLBACK_ABBR);
      expect(has, id).toBe(1);
    }
  });
  it('no map entry points at an unknown id', () => {
    for (const id of [
      ...Object.keys(SYSTEM_ICONS),
      ...Object.keys(FALLBACK_ABBR),
    ])
      expect(IDS, id).toContain(id);
  });
  it('no wordmark id is mapped to a silhouette', () => {
    for (const id of WORDMARK_IDS)
      expect(SYSTEM_ICONS, id).not.toHaveProperty(id);
  });
  it('bodies are clean', () => {
    for (const [id, icon] of Object.entries(SYSTEM_ICONS)) {
      expect(icon.body, id).not.toMatch(/#fff|<text|<image|style=/i);
      for (const m of icon.body.matchAll(/fill="([^"]*)"/g))
        expect(['currentColor', 'none'], id).toContain(m[1]);
    }
  });
  it('viewBoxes are valid and aspects sane', () => {
    for (const [id, icon] of Object.entries(SYSTEM_ICONS)) {
      expect(icon.viewBox, id).toMatch(/^-?[\d.]+ -?[\d.]+ [\d.]+ [\d.]+$/);
      expect(icon.aspect, id).toBeGreaterThanOrEqual(0.5);
      expect(icon.aspect, id).toBeLessThanOrEqual(2.5);
    }
  });
  it('every source entry is audited', () => {
    for (const [id, s] of Object.entries(SYSTEM_ICON_SOURCES)) {
      expect(s.depicts, id).toBeTruthy();
      expect(s.audit.length, id).toBeGreaterThan(5);
    }
  });
  it('abbreviations are 1–4 chars; user ids use their first four letters', () => {
    for (const v of Object.values(FALLBACK_ABBR)) expect(v).toMatch(/^.{1,4}$/);
    expect(abbreviationFor('x68000')).toBe('X680');
    expect(abbreviationFor('cps2')).toBe('CPS2');
  });
});
