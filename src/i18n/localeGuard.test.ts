import { describe, expect, it } from 'vitest';

import en from '../../locales/en/ui.json';
import es from '../../locales/es/ui.json';

/** Every string of a catalogue with its dotted key. */
function leaves(node: object, prefix = ''): [string, string][] {
  return Object.entries(node).flatMap(([key, value]): [string, string][] =>
    typeof value === 'string'
      ? [[`${prefix}${key}`, value]]
      : leaves(value as object, `${prefix}${key}.`),
  );
}

/** Every object (non-leaf) key of a catalogue, dotted. */
function branches(node: object, prefix = ''): string[] {
  return Object.entries(node).flatMap(([key, value]) =>
    typeof value === 'string'
      ? []
      : [`${prefix}${key}`, ...branches(value as object, `${prefix}${key}.`)],
  );
}

/**
 * No copy names an emulator or links anywhere (systems batch 3 D4): fake
 * PS5 and Xbox emulator repos spread malware, so Coverly states facts about
 * the file and never says what to install. Tanda 2's `Azahar`, `RPCS3` and
 * `Vita3K` reach the copy as `{emulator}` code constants, not locale text.
 * `http` is banned where this batch's copy lives (`organize.*`,
 * `systems.*`, `errors.large-transfer`): the proxy hint names the
 * `HTTPS_PROXY` / `HTTP_PROXY` variables, which are not links.
 */
const URL_SCOPES = ['organize.', 'systems.', 'errors.large-transfer'];
const BANNED = [
  'PCSX4',
  'PCSX5',
  'Rexbox',
  'KytyPS5',
  'SharpEmu',
  'AnyPS5',
  'Prosperity',
  'shadPS4',
  'WinDurango',
];

const CATALOGUES = [
  ['es', es],
  ['en', en],
] as const;

describe('locale copy guard', () => {
  it('locale values name no emulator and hold no URL', () => {
    for (const [lang, tree] of CATALOGUES)
      for (const [key, value] of leaves(tree)) {
        const text = value.toLowerCase();
        for (const word of BANNED)
          expect(
            text.includes(word.toLowerCase()),
            `${lang}:${key} has ${word}`,
          ).toBe(false);
        if (URL_SCOPES.some((scope) => key.startsWith(scope)))
          expect(text.includes('http'), `${lang}:${key} has http`).toBe(false);
        // A link is never copy, wherever it sits.
        expect(/https?:\/\/|www\./.test(text), `${lang}:${key}`).toBe(false);
      }
  });

  it('no key is both a string and an object', () => {
    for (const [lang, tree] of CATALOGUES) {
      const strings = new Set(leaves(tree).map(([k]) => k));
      for (const branch of branches(tree))
        expect(strings.has(branch), `${lang}:${branch}`).toBe(false);
      // Never a prefix of another leaf either (`a.b` beside `a.b.c`).
      for (const key of strings)
        expect(
          [...strings].some((other) => other.startsWith(`${key}.`)),
          `${lang}:${key}`,
        ).toBe(false);
    }
    // A path is a leaf in one tree exactly when it is a leaf in the other.
    const shape = (tree: object) => ({
      leaves: leaves(tree)
        .map(([k]) => k)
        .sort(),
      branches: branches(tree).sort(),
    });
    expect(shape(en)).toEqual(shape(es));
  });

  it('the batch 3 objects stay objects', () => {
    for (const [, tree] of CATALOGUES) {
      expect(branches(tree)).toContain('organize.largeTransfer');
      expect(branches(tree)).toContain('systems');
      const keys = leaves(tree).map(([k]) => k);
      expect(keys).toContain('organize.left.consoleSystem');
      expect(keys).toContain('systems.noEmulator');
      expect(keys).toContain('organize.why.contentId');
    }
  });
});
