import { render, screen } from '@testing-library/react';
import { ExplorerThemeProvider } from '@rumblefish/soroban-block-explorer-ui';
import { describe, expect, it, vi } from 'vitest';

import { LedgerTransactions } from './LedgerTransactions.js';

function renderSection(rows: [], totalCount: number) {
  return render(
    <ExplorerThemeProvider>
      <LedgerTransactions
        rows={rows}
        totalCount={totalCount}
        canPrev={false}
        canNext={false}
        onPrev={vi.fn()}
        onNext={vi.fn()}
      />
    </ExplorerThemeProvider>
  );
}

describe('LedgerTransactions empty state', () => {
  it('claims an empty ledger only when the header count agrees', () => {
    renderSection([], 0);

    expect(
      screen.getByText('This ledger closed without any transactions.')
    ).toBeInTheDocument();
  });

  // The header row is written before its transactions, so a recent ledger
  // legitimately reports a count with no rows yet. Calling that a load failure
  // was the regression this test exists to prevent (0377 F7).
  it('says "not indexed yet" — never "closed without any" — when the count is non-zero', () => {
    renderSection([], 42);

    expect(screen.getByText('Not indexed yet')).toBeInTheDocument();
    expect(
      screen.queryByText('This ledger closed without any transactions.')
    ).toBeNull();
  });

  it('does not blame a load failure for the indexing lag', () => {
    renderSection([], 42);

    expect(screen.queryByText(/could not be loaded/i)).toBeNull();
  });
});
