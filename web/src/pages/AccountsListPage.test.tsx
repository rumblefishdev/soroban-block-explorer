import type { AccountListItem } from '@rumblefish/api-types';
import { screen } from '@testing-library/react';
import { userEvent } from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

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
    first_seen_ledger: 10_000_000,
    last_seen_ledger: 54_000_000,
    home_domain: null,
    ...overrides,
  };
}

function mockOk(rows: AccountListItem[]): void {
  hookMocks.useAccountsList.mockReturnValue({
    data: {
      data: rows,
      page: { limit: 20, next_cursor: null, prev_cursor: null },
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
  it('renders the page header and the rows', () => {
    mockOk([
      makeRow({
        account_id: 'G' + 'B'.repeat(55),
        xlm_balance: '4107709533.0000000',
        home_domain: 'stellar.org',
      }),
      // Null native balance ⇒ the XLM cell renders a dash.
      makeRow({ account_id: 'G' + 'C'.repeat(55), xlm_balance: null }),
    ]);

    renderWithProviders(<AccountsListPage />, {
      initialEntries: ['/accounts'],
    });

    expect(
      screen.getByRole('heading', { level: 1, name: 'Accounts' })
    ).toBeInTheDocument();
    // Header row + two data rows render.
    expect(screen.getAllByRole('row')).toHaveLength(3);
    // home_domain renders as a chip next to the address.
    expect(screen.getByText('stellar.org')).toBeInTheDocument();
    // Null xlm_balance renders a dash.
    expect(screen.getByText('—')).toBeInTheDocument();
  });

  it('forwards the default order (desc) to the hook on first render', () => {
    mockOk([]);

    renderWithProviders(<AccountsListPage />, {
      initialEntries: ['/accounts'],
    });

    const lastCall =
      hookMocks.useAccountsList.mock.calls[
        hookMocks.useAccountsList.mock.calls.length - 1
      ];
    expect(lastCall?.[1]).toMatchObject({ limit: 20, order: 'desc' });
  });

  it('reads ?dir= from the URL and forwards it as the order', () => {
    mockOk([]);

    renderWithProviders(<AccountsListPage />, {
      initialEntries: ['/accounts?dir=asc'],
    });

    const lastCall =
      hookMocks.useAccountsList.mock.calls[
        hookMocks.useAccountsList.mock.calls.length - 1
      ];
    expect(lastCall?.[1]).toMatchObject({ order: 'asc' });
  });

  it('toggles the "With domain" filter into a hook call', async () => {
    mockOk([]);
    const user = userEvent.setup();

    renderWithProviders(<AccountsListPage />, {
      initialEntries: ['/accounts'],
    });

    let lastFilters =
      hookMocks.useAccountsList.mock.calls[
        hookMocks.useAccountsList.mock.calls.length - 1
      ]?.[1];
    expect(lastFilters?.['filter[with_domain]']).toBeUndefined();

    await user.click(screen.getByRole('button', { name: /with domain/i }));

    await vi.waitFor(() => {
      lastFilters =
        hookMocks.useAccountsList.mock.calls[
          hookMocks.useAccountsList.mock.calls.length - 1
        ]?.[1];
      expect(lastFilters?.['filter[with_domain]']).toBe(true);
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
