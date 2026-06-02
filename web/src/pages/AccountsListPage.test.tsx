import { screen } from '@testing-library/react';
import { userEvent } from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type {
  AccountListItem,
  AccountsListResponse,
} from '../api/hooks/useAccountsList.js';
import { renderWithProviders } from '../test-utils.js';

import AccountsListPage from './AccountsListPage.js';

const hookMocks = vi.hoisted(() => ({
  useAccountsList: vi.fn(),
}));

vi.mock('../api/hooks/useAccountsList.js', () => ({
  useAccountsList: hookMocks.useAccountsList,
}));

function makeRow(overrides: Partial<AccountListItem> = {}): AccountListItem {
  return {
    account_id: 'G' + 'A'.repeat(55),
    xlm_balance: '1000.0000000',
    xlm_supply_percent: 0.5,
    first_seen_ledger: 10_000_000,
    last_seen_ledger: 54_000_000,
    home_domain: null,
    ...overrides,
  };
}

function mockOk(
  rows: AccountListItem[],
  page?: Partial<AccountsListResponse['page']>
): void {
  hookMocks.useAccountsList.mockReturnValue({
    data: {
      data: rows,
      page: { limit: 20, next_cursor: null, prev_cursor: null, ...page },
    },
    isLoading: false,
    isError: false,
    error: null,
    refetch: vi.fn(),
  });
}

beforeEach(() => {
  hookMocks.useAccountsList.mockReset();
});

afterEach(() => {
  vi.clearAllMocks();
});

describe('AccountsListPage', () => {
  it('renders the page header and a row count', () => {
    mockOk([
      makeRow({
        account_id: 'G' + 'B'.repeat(55),
        xlm_balance: '4107709533.0000000',
        xlm_supply_percent: 8.22,
        home_domain: 'stellar.org',
      }),
      makeRow({
        account_id: 'G' + 'C'.repeat(55),
        xlm_balance: '3779092770.0000000',
        xlm_supply_percent: 7.56,
      }),
    ]);

    renderWithProviders(<AccountsListPage />, {
      initialEntries: ['/accounts'],
    });

    expect(
      screen.getByRole('heading', { level: 1, name: 'Accounts' })
    ).toBeInTheDocument();
    // Rank column starts at 1 on the first page.
    expect(screen.getByText('1')).toBeInTheDocument();
    expect(screen.getByText('2')).toBeInTheDocument();
    expect(screen.getByText('stellar.org')).toBeInTheDocument();
  });

  it('passes the default sort (xlm_desc) to the hook on first render', () => {
    mockOk([]);

    renderWithProviders(<AccountsListPage />, {
      initialEntries: ['/accounts'],
    });

    const lastCall =
      hookMocks.useAccountsList.mock.calls[
        hookMocks.useAccountsList.mock.calls.length - 1
      ];
    const filtersArg = lastCall?.[1];
    expect(filtersArg).toMatchObject({ limit: 20, sort: 'xlm_desc' });
  });

  it('reads the split ?sort=&dir= params and forwards the recombined sort', () => {
    mockOk([]);

    // Sort lives split across `sort` (column) + `dir` (direction); the page
    // recombines them into the `<column>_<dir>` token the API expects.
    renderWithProviders(<AccountsListPage />, {
      initialEntries: ['/accounts?sort=last_seen&dir=desc'],
    });

    const lastCall =
      hookMocks.useAccountsList.mock.calls[
        hookMocks.useAccountsList.mock.calls.length - 1
      ];
    expect(lastCall?.[1]).toMatchObject({ sort: 'last_seen_desc' });
  });

  it('toggles the "With domain" filter into a hook call', async () => {
    mockOk([]);
    const user = userEvent.setup();

    renderWithProviders(<AccountsListPage />, {
      initialEntries: ['/accounts'],
    });

    // First render: no with_domain key.
    let lastFilters =
      hookMocks.useAccountsList.mock.calls[
        hookMocks.useAccountsList.mock.calls.length - 1
      ]?.[1];
    expect(lastFilters?.with_domain).toBeUndefined();

    await user.click(screen.getByRole('button', { name: /with domain/i }));

    await vi.waitFor(() => {
      lastFilters =
        hookMocks.useAccountsList.mock.calls[
          hookMocks.useAccountsList.mock.calls.length - 1
        ]?.[1];
      expect(lastFilters?.with_domain).toBe(true);
    });
  });

  it('renders the entity-specific empty state when no rows and no filters', () => {
    mockOk([]);

    renderWithProviders(<AccountsListPage />, {
      initialEntries: ['/accounts'],
    });

    expect(screen.getByText(/no accounts found/i)).toBeInTheDocument();
  });
});
