import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';
import type { GameDetailsView, RegionVerdictView } from '@/ipc/types';

import { RegionLine } from './RegionLine';

const details = (region?: RegionVerdictView): GameDetailsView => ({
  request: 1,
  title_id: '0004000000055E00',
  product_code: region?.code ?? 'CTR-P-AXXE',
  ...(region ? { region } : {}),
  children: [],
});

function line(d: GameDetailsView | null, nameHasRegion = false) {
  return render(
    <I18nProvider locale="es">
      <RegionLine systemName="3DS" nameHasRegion={nameHasRegion} details={d} />
    </I18nProvider>,
  );
}

describe('RegionLine (addon views §8.8 1)', () => {
  it('names the region and the country a country letter gives', () => {
    line(
      details({
        letter: 'S',
        code: 'CTR-P-AXXS',
        region: 'europe',
        country: 'spain',
        certainty: 'certain',
      }),
    );
    expect(screen.getByText('3DS, Europa, España')).toBeInTheDocument();
    // The title id, in mono, with its name for screen readers.
    const tid = screen.getByText('0004000000055E00');
    expect(tid).toHaveClass('mono');
    expect(tid.parentElement).toHaveTextContent(
      'ID de título: 0004000000055E00',
    );
  });

  it('says a likely region is likely, with its sources', () => {
    line(
      details({
        letter: 'Z',
        code: 'CTR-P-AXXZ',
        region: 'europe',
        certainty: 'likely',
      }),
    );
    expect(
      screen.getByText('3DS, Europa (probable, según GameTDB y 3dbrew)'),
    ).toBeInTheDocument();
  });

  it('says a region-only letter without a country', () => {
    line(
      details({
        letter: 'P',
        code: 'CTR-P-AXXP',
        region: 'europe',
        certainty: 'certain',
      }),
    );
    expect(screen.getByText('3DS, Europa')).toBeInTheDocument();
  });

  it('flags a letter the table does not know, never guessing', () => {
    line(details({ letter: 'T', code: 'CTR-P-AXXT', certainty: 'unknown' }));
    const warn = screen.getByText(
      'Letra de región «T» sin identificar (CTR-P-AXXT)',
    );
    expect(warn).toHaveClass('panel-head__unknown');
    expect(screen.queryByText(/^3DS,/)).toBeNull();
  });

  it('shows no region when the name already carries one', () => {
    line(
      details({
        letter: 'T',
        code: 'CTR-P-AXXT',
        certainty: 'unknown',
      }),
      true,
    );
    expect(screen.queryByText(/sin identificar/)).toBeNull();
    expect(screen.getByText('0004000000055E00')).toBeInTheDocument();
  });

  it('renders nothing when the details did not load', () => {
    const { container } = line(null);
    expect(container).toBeEmptyDOMElement();
  });
});
