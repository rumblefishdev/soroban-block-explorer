import type { ParticipantItem } from '@rumblefish/api-types';
import { screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { renderWithProviders } from '../../../test-utils.js';

import { PoolParticipants } from '../PoolParticipants.js';

const hookMock = vi.hoisted(() => ({ usePoolParticipants: vi.fn() }));

vi.mock('../../../api/index.js', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../../api/index.js')>()),
  usePoolParticipants: hookMock.usePoolParticipants,
}));

const ACCOUNT = 'GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN';
const GAUGE = 'CAQCFVLOBK5GIULPNZRGSXFPMIDUTBDDKCEHQNCZGYNK5JEN6IY5RZQB';

function mockQuery(state: {
  rows?: ParticipantItem[];
  error?: { body?: { code?: string } };
}) {
  hookMock.usePoolParticipants.mockReturnValue({
    data: state.rows ? { data: state.rows, page: { limit: 20 } } : undefined,
    isLoading: false,
    isPlaceholderData: false,
    isError: state.error != null,
    error: state.error ?? null,
    refetch: vi.fn(),
  });
}

describe('PoolParticipants', () => {
  it('says "not indexed" for a pool the API reports as not indexed', () => {
    // The API's 400 `code` is the whole contract here: a concentrated
    // soroban pool keeps positions, not a share token.
    mockQuery({ error: { body: { code: 'not_indexed' } } });
    renderWithProviders(<PoolParticipants poolId="CPOOL" />);

    expect(screen.getByText('Not indexed yet')).toBeInTheDocument();
    expect(screen.queryByText('No participants yet')).not.toBeInTheDocument();
  });

  it('shows a soroban provider with no first deposit and links a contract holder', () => {
    mockQuery({
      rows: [
        {
          account: ACCOUNT,
          shares: '300',
          share_percentage: '75',
          first_deposit_ledger: null,
          last_updated_ledger: 20,
        },
        {
          account: GAUGE,
          shares: '100',
          share_percentage: '25',
          first_deposit_ledger: null,
          last_updated_ledger: 30,
        },
      ],
    });
    renderWithProviders(<PoolParticipants poolId="CPOOL" />);

    const table = within(screen.getByRole('table'));
    // Share-token balances record no first deposit: a dash, never a ledger 0.
    expect(table.getAllByText('—')).toHaveLength(2);
    expect(table.queryByText('0')).not.toBeInTheDocument();
    // A gauge or vault holding the share token links to its contract page.
    const hrefs = table.getAllByRole('link').map((a) => a.getAttribute('href'));
    expect(hrefs).toContain(`/contracts/${GAUGE}`);
    expect(hrefs).toContain(`/accounts/${ACCOUNT}`);
  });
});
