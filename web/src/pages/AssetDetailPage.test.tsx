import type { AssetDetailResponse } from '@rumblefish/api-types';
import { screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { renderWithProviders } from '../test-utils.js';

import AssetDetailPage from './AssetDetailPage.js';

const hookMocks = vi.hoisted(() => ({
  useAssetDetail: vi.fn(),
  useAssetTransactions: vi.fn(),
}));

vi.mock('../api/index.js', () => ({
  useAssetDetail: hookMocks.useAssetDetail,
  useAssetTransactions: hookMocks.useAssetTransactions,
}));

function makeAsset(
  overrides: Partial<AssetDetailResponse> = {}
): AssetDetailResponse {
  return {
    id: 'native',
    asset_code: 'XLM',
    asset_type: 0,
    asset_type_name: 'native',
    issuer: null,
    contract_id: null,
    holder_count: 1_000_000,
    total_supply: '50000000.0000000',
    icon_url: null,
    name: null,
    description: null,
    home_page: null,
    deployed_at_ledger: null,
    ...overrides,
  };
}

function mockOk(asset: AssetDetailResponse): void {
  hookMocks.useAssetDetail.mockReturnValue({
    data: asset,
    isLoading: false,
    isError: false,
    error: null,
    refetch: vi.fn(),
  });
}

beforeEach(() => {
  hookMocks.useAssetDetail.mockReset();
  hookMocks.useAssetTransactions.mockReturnValue({
    data: { data: [], page: { limit: 20 } },
    isLoading: false,
    isError: false,
    error: null,
    refetch: vi.fn(),
  });
});

afterEach(() => {
  vi.clearAllMocks();
});

describe('AssetDetailPage', () => {
  it('renders the heading and the "Native" badge for native XLM', () => {
    mockOk(makeAsset({ asset_code: 'XLM', asset_type_name: 'native' }));

    renderWithProviders(<AssetDetailPage />, {
      initialEntries: ['/assets/native'],
      routePath: '/assets/:id',
    });

    expect(
      screen.getByRole('heading', { level: 1, name: 'XLM' })
    ).toBeInTheDocument();
    expect(screen.getByText('Native')).toBeInTheDocument();
  });

  it.each([
    ['classic_credit', 'Classic'],
    ['sac', 'SAC'],
    ['soroban', 'Soroban'],
  ])('renders the "%s" type with badge "%s"', (typeName, label) => {
    mockOk(
      makeAsset({
        asset_code: 'USDC',
        asset_type: 1,
        asset_type_name: typeName,
        issuer: 'GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN',
      })
    );

    renderWithProviders(<AssetDetailPage />, {
      initialEntries: ['/assets/USDC-XYZ'],
      routePath: '/assets/:id',
    });

    expect(screen.getByText(label)).toBeInTheDocument();
  });

  it('renders the asset name sub-line when present', () => {
    mockOk(
      makeAsset({
        asset_code: 'USDC',
        asset_type: 1,
        asset_type_name: 'classic_credit',
        name: 'USD Coin',
      })
    );

    renderWithProviders(<AssetDetailPage />, {
      initialEntries: ['/assets/USDC-XYZ'],
      routePath: '/assets/:id',
    });

    // Name is rendered in both the header sub-line and the Summary
    // card — assert at least one match.
    expect(screen.getAllByText('USD Coin').length).toBeGreaterThan(0);
  });

  it('renders cleanly when TOML metadata fields are null (partial-metadata tolerance)', () => {
    mockOk(
      makeAsset({
        asset_code: 'EURC',
        asset_type: 1,
        asset_type_name: 'classic_credit',
        // All TOML fields null — the metadata card should still render
        // without throwing.
        name: null,
        description: null,
        home_page: null,
      })
    );

    renderWithProviders(<AssetDetailPage />, {
      initialEntries: ['/assets/EURC-XYZ'],
      routePath: '/assets/:id',
    });

    expect(
      screen.getByRole('heading', { level: 1, name: 'EURC' })
    ).toBeInTheDocument();
    expect(screen.getByText('Classic')).toBeInTheDocument();
  });

  it('renders NotFoundState when the asset query 404s', () => {
    hookMocks.useAssetDetail.mockReturnValue({
      data: undefined,
      isLoading: false,
      isError: true,
      error: Object.assign(new Error('not found'), { status: 404 }),
      refetch: vi.fn(),
    });

    renderWithProviders(<AssetDetailPage />, {
      initialEntries: ['/assets/MISSING-XYZ'],
      routePath: '/assets/:id',
    });

    expect(screen.getByText('Asset not found')).toBeInTheDocument();
  });
});
