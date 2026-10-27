import { render } from '@testing-library/react';
import type { ReactNode } from 'react';
import { describe, expect, it } from 'vitest';

import type { TranslateVars } from './core';
import { I18nProvider } from './I18nProvider';
import { useRichT } from './rich';

function Probe({
  k,
  vars,
  mono,
}: {
  k: string;
  vars?: TranslateVars;
  mono?: readonly string[];
}) {
  const tr = useRichT();
  return <p data-testid="probe">{tr(k, vars, mono)}</p>;
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
});
