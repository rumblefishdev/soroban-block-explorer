import { screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { renderWithProviders } from '../../test-utils.js';

import { TransactionCounts } from './TransactionCounts.js';

describe('TransactionCounts', () => {
  it('keeps the total as the primary value and states the failure rate', () => {
    renderWithProviders(<TransactionCounts total={412} successful={319} />);

    expect(screen.getByText('412')).toBeInTheDocument();
    expect(screen.getByText('22.6% failed')).toBeInTheDocument();
  });

  it('labels both numbers for screen readers', () => {
    // Without this the cell announces bare integers under a header that says
    // only "Transactions".
    renderWithProviders(<TransactionCounts total={412} successful={319} />);

    expect(
      screen.getByLabelText('412 transactions, 319 succeeded, 93 failed')
    ).toBeInTheDocument();
  });

  it('says the split is unavailable rather than implying a total failure', () => {
    renderWithProviders(<TransactionCounts total={365} successful={null} />);

    expect(screen.getByText('365')).toBeInTheDocument();
    expect(screen.getByText('split unavailable')).toBeInTheDocument();
    expect(screen.queryByText(/failed/)).not.toBeInTheDocument();
  });

  it('treats undefined like null — the generated type omits the field', () => {
    renderWithProviders(
      <TransactionCounts total={365} successful={undefined} />
    );

    expect(screen.getByText('split unavailable')).toBeInTheDocument();
  });

  it('reports an all-failed ledger as 100%, not as missing data', () => {
    renderWithProviders(<TransactionCounts total={12} successful={0} />);

    expect(screen.getByText('100.0% failed')).toBeInTheDocument();
  });

  it('reports a flawless ledger as 0%', () => {
    renderWithProviders(<TransactionCounts total={12} successful={12} />);

    expect(screen.getByText('0.0% failed')).toBeInTheDocument();
  });

  it('drops the split when the two sources disagree', () => {
    // `successful > total` is impossible; the sources are different tables, so
    // admit the split is untrustworthy rather than clamping to a plausible lie.
    renderWithProviders(<TransactionCounts total={10} successful={12} />);

    expect(screen.getByText('10')).toBeInTheDocument();
    expect(screen.getByText('split unavailable')).toBeInTheDocument();
  });

  it('groups four-digit totals', () => {
    renderWithProviders(<TransactionCounts total={1171} successful={1000} />);

    expect(screen.getByText('1,171')).toBeInTheDocument();
  });

  it('colours the rate only when failures dominate', () => {
    const calm = renderWithProviders(
      <TransactionCounts total={100} successful={70} />
    );
    const calmColour = getComputedStyle(screen.getByText('30.0% failed')).color;
    calm.unmount();

    renderWithProviders(<TransactionCounts total={100} successful={30} />);
    const alarmedColour = getComputedStyle(
      screen.getByText('70.0% failed')
    ).color;

    expect(calmColour).not.toBe(alarmedColour);
  });
});
