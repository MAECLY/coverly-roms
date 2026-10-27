import { render } from '@testing-library/react';
import type { ReactNode } from 'react';
import { describe, expect, it } from 'vitest';

import { translate, type TranslateVars } from './core';
import { I18nProvider } from './I18nProvider';
import { useRichT } from './rich';

function Probe({
  k,
  vars,
  mono,
  titles,
}: {
  k: string;
  vars?: TranslateVars;
  mono?: readonly string[];
  titles?: Readonly<Record<string, string>>;
}) {
  const tr = useRichT();
  return <p data-testid="probe">{tr(k, vars, mono, titles)}</p>;
}

function renderEs(node: ReactNode) {
  return render(<I18nProvider locale="es">{node}</I18nProvider>);
}

const monoTexts = (root: HTMLElement) =>
  Array.from(root.querySelectorAll('span.mono.tabular-nums')).map(
    (el) => el.textContent,
  );

describe('useRichT', () => {
  it('interpolates and wraps every value in a mono tabular span', () => {
    const { getByTestId } = renderEs(
      <Probe k="organize.progress.track" vars={{ n: 4, total: 12 }} />,
    );
    const p = getByTestId('probe');
    expect(p.textContent).toBe('Pista 4 de 12');
    expect(monoTexts(p)).toEqual(['4', '12']);
  });

  it('does not wrap literal text', () => {
    const { getByTestId } = renderEs(
      <Probe k="organize.progress.track" vars={{ n: 4, total: 12 }} />,
    );
    const spans = getByTestId('probe').querySelectorAll('span');
    for (const span of spans) {
      expect(span.textContent).not.toMatch(/Pista|de/);
    }
  });

  it('leaves an unknown placeholder literal', () => {
    const { getByTestId } = renderEs(
      <Probe k="organize.progress.track" vars={{ n: 4 }} />,
    );
    const p = getByTestId('probe');
    expect(p.textContent).toBe('Pista 4 de {total}');
    expect(monoTexts(p)).toEqual(['4']);
  });

  it('wraps only the named values when a mono list is given', () => {
    const { getByTestId } = renderEs(
      <Probe
        k="organize.progress.extracting"
        vars={{ name: 'Next Tetris, The.zip', folder: 'PS1' }}
        mono={['name']}
      />,
    );
    const p = getByTestId('probe');
    expect(p.textContent).toBe('Extrayendo Next Tetris, The.zip en PS1');
    expect(monoTexts(p)).toEqual(['Next Tetris, The.zip']);
    expect(p.querySelectorAll('bdi')).toHaveLength(2);
  });

  it('puts a value title on its own element only', () => {
    const { getByTestId } = renderEs(
      <Probe
        k="organize.progress.extracting"
        vars={{ name: 'Tetris.zip', folder: 'PS…Game' }}
        titles={{ folder: '/Volumes/SD/Roms/PS/Game' }}
      />,
    );
    const p = getByTestId('probe');
    const titled = p.querySelectorAll('[title]');
    expect(titled).toHaveLength(1);
    expect(titled[0]!.textContent).toBe('PS…Game');
    expect(titled[0]!.getAttribute('title')).toBe('/Volumes/SD/Roms/PS/Game');
  });
});

// Spec §7 (plus the plan's `movingLabel`): parity only proves es == en, so a
// key missing from both would slip through until a screen shows the raw key.
const PROGRESS_KEYS = [
  ...[
    'extracting',
    'checking',
    'game',
    'track',
    'file',
    'amount',
    'rate',
    'leftMinutes',
    'leftUnderMinute',
    'leftOverHour',
    'leftOverHours',
    'measuring',
    'cancelling',
    'cancellingButton',
    'tracksLabel',
    'filesLabel',
    'showTracks',
    'showFiles',
    'waiting',
    'writing',
    'crcChecked',
    'sizeChecked',
    'cancelNote',
    'stageLabel',
    'movingLabel',
    'extractingName',
  ].map((k) => `organize.progress.${k}`),
  'start.progress.folders',
  'progress.perMinute',
];

describe('progress copy', () => {
  it('resolves every key of spec §7 in both locales without exclamation marks', () => {
    for (const locale of ['es', 'en'] as const) {
      for (const key of PROGRESS_KEYS) {
        const text = translate(locale, key);
        expect(text, `${locale} ${key}`).not.toBe(key);
        expect(text, `${locale} ${key}`).not.toMatch(/[!¡]/);
      }
    }
  });
});
