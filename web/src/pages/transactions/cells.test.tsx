import { screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { renderWithProviders } from '../../test-utils.js';

import { ValueCell } from './cells.js';

// First ledger with a live-indexed `net_settled` on prod (see cells.tsx).
const FLOOR = 63_699_653;

function renderCell(props: Parameters<typeof ValueCell>[0]) {
  return renderWithProviders(<ValueCell {...props} />);
}

describe('ValueCell', () => {
  it('renders n/a for a pre-backfill transaction with no values', () => {
    renderCell({ values: [], ledgerSequence: FLOOR - 1 });
    expect(screen.getByText('n/a')).toBeInTheDocument();
  });

  it('renders a dash for a live-indexed transaction with no values', () => {
    renderCell({ values: [], ledgerSequence: FLOOR });
    expect(screen.queryByText('n/a')).not.toBeInTheDocument();
  });

  it('renders the scaled amount and code when values are present', () => {
    renderCell({
      values: [
        {
          asset: 'native',
          asset_code: null,
          net_settled: '25000000000',
          decimals: 7,
        },
      ],
      ledgerSequence: FLOOR + 1,
    });
    expect(screen.getByText('2,500.00')).toBeInTheDocument();
    expect(screen.getByText('XLM')).toBeInTheDocument();
  });
});
