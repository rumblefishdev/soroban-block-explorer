import type { AssetItem, PaginatedAssetItem } from '@rumblefish/api-types';
import { screen } from '@testing-library/react';
import { userEvent } from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { renderWithProviders } from '../test-utils.js';

import AssetsListPage from './AssetsListPage.js';

const assetsHookMock = vi.hoisted(() => ({
  useAssetsList: vi.fn(),
}));

vi.mock('../api/hooks/useAssetsList.js', () => ({
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
    // RAW Int128 (task 0331 Option C) — 50,000,000 scaled by decimals=7.
    total_supply: '500000000000000',
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

  it('renders the filtered-empty CTA when zero rows come back with the SAC filter', () => {
    mockOk([]);

    renderWithProviders(<AssetsListPage />, {
      initialEntries: ['/assets?sac=true'],
    });

    expect(
      screen.getByText(/no assets match your filters/i)
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: /clear filters/i })
    ).toBeInTheDocument();
  });

  it('maps the "has SAC" property filter to filter[sac]=true', () => {
    mockOk([]);

    renderWithProviders(<AssetsListPage />, {
      initialEntries: ['/assets?sac=true'],
    });

    const calls = assetsHookMock.useAssetsList.mock.calls;
    const lastFilters = calls[calls.length - 1]?.[1];
    expect(lastFilters?.['filter[sac]']).toBe('true');
    expect(lastFilters?.['filter[type]']).toBeUndefined();
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

  it('selecting Soroban clears an active "Has SAC" filter (mutually exclusive)', async () => {
    mockOk([]);
    const user = userEvent.setup();

    renderWithProviders(<AssetsListPage />, {
      initialEntries: ['/assets?sac=true'],
    });

    await user.click(screen.getByRole('button', { name: 'Soroban' }));

    await vi.waitFor(() => {
      const calls = assetsHookMock.useAssetsList.mock.calls;
      const f = calls[calls.length - 1]?.[1];
      expect(f?.['filter[type]']).toBe('soroban');
      expect(f?.['filter[sac]']).toBeUndefined();
    });
  });

  it('turning on "Has SAC" while Soroban is active drops the type', async () => {
    mockOk([]);
    const user = userEvent.setup();

    renderWithProviders(<AssetsListPage />, {
      initialEntries: ['/assets?type=soroban'],
    });

    await user.click(screen.getByRole('button', { name: 'Has SAC' }));

    await vi.waitFor(() => {
      const calls = assetsHookMock.useAssetsList.mock.calls;
      const f = calls[calls.length - 1]?.[1];
      expect(f?.['filter[sac]']).toBe('true');
      expect(f?.['filter[type]']).toBeUndefined();
    });
  });

  it('ignores a stale filter[sac] from a deep link when type=soroban', () => {
    mockOk([]);

    renderWithProviders(<AssetsListPage />, {
      initialEntries: ['/assets?type=soroban&sac=true'],
    });

    const calls = assetsHookMock.useAssetsList.mock.calls;
    const f = calls[calls.length - 1]?.[1];
    expect(f?.['filter[type]']).toBe('soroban');
    expect(f?.['filter[sac]']).toBeUndefined();
  });
});
