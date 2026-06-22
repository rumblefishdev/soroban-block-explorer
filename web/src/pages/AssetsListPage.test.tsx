import type { AssetItem, PaginatedAssetItem } from '@rumblefish/api-types';
import { screen } from '@testing-library/react';
import { userEvent } from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { renderWithProviders } from '../test-utils.js';

import AssetsListPage from './AssetsListPage.js';

const assetsHookMock = vi.hoisted(() => ({
  useAssetsList: vi.fn(),
}));

vi.mock('../api/index.js', () => ({
  useAssetsList: assetsHookMock.useAssetsList,
}));

function makeAsset(overrides: Partial<AssetItem> = {}): AssetItem {
  return {
    id: 'native',
    asset_code: 'XLM',
    asset_type: 0,
    asset_type_name: 'native',
    decimals: 7,
    issuer: null,
    contract_id: null,
    holder_count: 1_000_000,
    total_supply: '50000000.0000000',
    icon_url: null,
    name: 'Stellar Lumens',
    ...overrides,
  };
}

type Page = NonNullable<PaginatedAssetItem['page']>;

function mockOk(rows: AssetItem[], page?: Partial<Page>): void {
  assetsHookMock.useAssetsList.mockReturnValue({
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
  assetsHookMock.useAssetsList.mockReset();
});

afterEach(() => {
  vi.clearAllMocks();
});

describe('AssetsListPage', () => {
  it('renders the page header and the rows from a successful query', () => {
    mockOk([
      makeAsset({ id: 'native', asset_code: 'XLM', asset_type_name: 'native' }),
      makeAsset({
        id: 'USDC-GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN',
        asset_code: 'USDC',
        asset_type: 1,
        asset_type_name: 'classic_credit',
        issuer: 'GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN',
        name: 'USD Coin',
      }),
    ]);

    renderWithProviders(<AssetsListPage />, { initialEntries: ['/assets'] });

    expect(
      screen.getByRole('heading', { level: 1, name: 'Assets' })
    ).toBeInTheDocument();
    expect(screen.getAllByText('XLM').length).toBeGreaterThan(0);
    expect(screen.getAllByText('USDC').length).toBeGreaterThan(0);
  });

  it('renders the empty state when zero rows come back with no filters set', () => {
    mockOk([]);

    renderWithProviders(<AssetsListPage />, { initialEntries: ['/assets'] });

    expect(screen.getByText(/no tokens found/i)).toBeInTheDocument();
  });

  it('renders the filtered-empty CTA when zero rows come back with a type filter', () => {
    mockOk([]);

    renderWithProviders(<AssetsListPage />, {
      initialEntries: ['/assets?type=sac'],
    });

    expect(
      screen.getByText(/no assets match your filters/i)
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: /clear filters/i })
    ).toBeInTheDocument();
  });

  it('typing in the code filter drives the next hook call', async () => {
    mockOk([]);
    const user = userEvent.setup();

    renderWithProviders(<AssetsListPage />, { initialEntries: ['/assets'] });

    // MUI TextField doesn't expose `aria-label` as the input's
    // accessible name — match by placeholder instead.
    const search = screen.getByPlaceholderText(/search by asset code/i);
    await user.type(search, 'USDC');

    // The page debounces typing — wait for the hook to be called with the
    // filter applied.
    await vi.waitFor(() => {
      const calls = assetsHookMock.useAssetsList.mock.calls;
      const lastFilters = calls[calls.length - 1]?.[1];
      expect(lastFilters?.['filter[code]']).toBe('USDC');
    });
  });
});
