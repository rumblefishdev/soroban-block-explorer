import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { TransactionCounts } from './TransactionCounts.js';

describe('TransactionCounts', () => {
  it('splits the total into successful and failed', () => {
    render(<TransactionCounts total={365} successful={280} />);

    expect(screen.getByText('280')).toBeInTheDocument();
    expect(screen.getByText('85')).toBeInTheDocument();
    expect(screen.queryByText('365')).not.toBeInTheDocument();
  });

  it('renders the plain total when the split is unavailable', () => {
    // The failure this guards: rendering a missing aggregate as `0 successful`
    // would claim a ledger in which every transaction failed.
    render(<TransactionCounts total={365} successful={null} />);

    expect(screen.getByText('365')).toBeInTheDocument();
    expect(screen.queryByText('0')).not.toBeInTheDocument();
  });

  it('shows an all-failed ledger as zero successful, not as missing data', () => {
    render(<TransactionCounts total={12} successful={0} />);

    expect(screen.getByText('0')).toBeInTheDocument();
    expect(screen.getByText('12')).toBeInTheDocument();
  });

  it('never renders a negative failed count if the two sources drift', () => {
    render(<TransactionCounts total={10} successful={12} />);

    expect(screen.getByText('12')).toBeInTheDocument();
    expect(screen.getByText('0')).toBeInTheDocument();
  });
});
