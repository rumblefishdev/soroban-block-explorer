import { screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { renderWithProviders } from '../../../test-utils.js';
import { SponsoredReserves } from '../SponsoredReserves.js';

describe('SponsoredReserves', () => {
  it('shows a wallet’s reserves paid for others with their XLM', () => {
    // GAUA7…PNJU on mainnet, 2026-10-07.
    renderWithProviders(
      <SponsoredReserves
        sponsorship={{ num_sponsoring: 4_051_651, num_sponsored: 0 }}
      />
    );

    expect(screen.getByText(/4,051,651 reserves/)).toBeInTheDocument();
    expect(
      screen.getByText(/2,025,825\.5 XLM locked for others/)
    ).toBeInTheDocument();
    expect(screen.getByText('0 reserves')).toBeInTheDocument();
  });

  it('shows a sponsored account’s reserves paid by its sponsor', () => {
    // GA3WEM…UV76: account (2) + one trustline (1), all paid by a wallet.
    renderWithProviders(
      <SponsoredReserves
        sponsorship={{ num_sponsoring: 0, num_sponsored: 3 }}
      />
    );

    expect(screen.getByText(/1\.5 XLM paid by sponsors/)).toBeInTheDocument();
  });
});
