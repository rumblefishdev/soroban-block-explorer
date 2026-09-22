import { screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { renderWithProviders } from '../../../test-utils.js';

import { PoolParticipants } from '../PoolParticipants.js';

const hookMock = vi.hoisted(() => ({ usePoolParticipants: vi.fn() }));

vi.mock('../../../api/index.js', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../../api/index.js')>()),
  usePoolParticipants: hookMock.usePoolParticipants,
}));

function returns(data: unknown[]) {
  hookMock.usePoolParticipants.mockReturnValue({
    data: { data, page: { limit: 20 } },
    isLoading: false,
    isPlaceholderData: false,
    isError: false,
    error: null,
    refetch: vi.fn(),
  });
}

describe('PoolParticipants empty state', () => {
  beforeEach(() => returns([]));

  // Each of the three counts is a different fact, and only one of them is
  // "nobody provides liquidity here".
  it('says none only when the count is a measured zero', () => {
    renderWithProviders(<PoolParticipants poolId="C1" knownParticipants={0} />);
    expect(screen.getByText('No participants yet')).toBeInTheDocument();
  });

  it('says counted but not listed when providers exist', () => {
    renderWithProviders(
      <PoolParticipants poolId="C1" knownParticipants={136} />
    );
    expect(screen.getByText('Participants not listed')).toBeInTheDocument();
  });

  it('says not indexed when the count itself is unknown', () => {
    renderWithProviders(
      <PoolParticipants poolId="C1" knownParticipants={null} />
    );
    expect(screen.getByText('Participants not indexed')).toBeInTheDocument();
    expect(screen.queryByText('No participants yet')).not.toBeInTheDocument();
  });
});

describe('PoolParticipants soroban holders', () => {
  const account = 'GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN';
  const contract = 'CAQCFVLOBK5GIULPNZRGSXFPMIDUTBDDKCEHQNCZGYNK5JEN6IY5RZQB';

  // A share token held by a locker contract is the normal case, and a token
  // with no published scale has no honest amount to show.
  it('lists a contract holder and dashes an unscaled amount', () => {
    returns([
      {
        account,
        shares: '12.5',
        share_percentage: '60.25',
        first_deposit_ledger: 60_000_000,
        last_updated_ledger: 61_000_000,
      },
      {
        account: contract,
        shares: null,
        share_percentage: '39.75',
        first_deposit_ledger: 60_000_100,
        last_updated_ledger: 61_000_100,
      },
    ]);
    renderWithProviders(<PoolParticipants poolId="C1" knownParticipants={2} />);

    expect(
      screen
        .getAllByRole('link')
        .some((a) => a.getAttribute('href')?.includes(`/contracts/${contract}`))
    ).toBe(true);
    expect(screen.getByText('—')).toBeInTheDocument();
    expect(
      screen.queryByText('Participants not listed')
    ).not.toBeInTheDocument();
  });
});
