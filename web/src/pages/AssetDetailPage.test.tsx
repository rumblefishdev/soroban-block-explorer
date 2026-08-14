import type { AssetDetailResponse } from '@rumblefish/api-types';
import { screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { renderWithProviders } from '../test-utils.js';

import AssetDetailPage from './AssetDetailPage.js';

// A real G-strkey issuer so `/assets/CODE-ISSUER` URLs pass the page's
// `isAssetId` pre-validation (the route param is canonical post-0243).
const ISSUER = 'GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN';
// A SAC contract StrKey (`C…`) for exercising the SAC facet tag/link.
const SAC_CONTRACT = 'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75';

const hookMocks = vi.hoisted(() => ({
  useAssetDetail: vi.fn(),
  useAssetTransactions: vi.fn(),
}));

vi.mock('../api/index.js', async () => ({
  useAssetDetail: hookMocks.useAssetDetail,
  useAssetTransactions: hookMocks.useAssetTransactions,
  // Pure pagination helper — keep the real implementation so the
  // transactions section renders instead of hitting its error boundary.
  usePagedRows: (
    await vi.importActual<typeof import('../api/usePagedRows.js')>(
      '../api/usePagedRows.js'
    )
  ).usePagedRows,
}));

function makeAsset(
  overrides: Partial<AssetDetailResponse> = {}
): AssetDetailResponse {
  return {
    id: 'native',
    // Native carries NO asset_code on the ledger — the API returns null, and
    // the display rule (`assetDisplayCode`) is what names it XLM (0472). The
    // fixture used to hand it 'XLM', which hid the real gap from the tests.
    asset_code: null,
    asset_type: 0,
    asset_type_name: 'native',
    decimals: 7,
    issuer: null,
    contract_id: null,
    holder_count: 1_000_000,
    // RAW Int128 (task 0331 Option C) — 50,000,000 scaled by decimals=7.
    total_supply: '500000000000000',
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
  it('names native XLM by its type, not its (absent) code — title and avatar', () => {
    // The real /assets/native payload: asset_type_name 'native', no code and
    // no symbol. Before 0472 this rendered the title "Asset" and a "?" avatar.
    mockOk(makeAsset({ asset_type_name: 'native' }));

    renderWithProviders(<AssetDetailPage />, {
      initialEntries: ['/assets/native'],
      routePath: '/assets/:id',
    });

    expect(
      screen.getByRole('heading', { level: 1, name: 'XLM' })
    ).toBeInTheDocument();
    // The letter avatar takes the same label, so it reads "X", never "?".
    // Two avatars carry it since 0472 unified the metadata card onto the same
    // rule: the header and the TOML "Icon" row.
    expect(screen.getAllByText('X').length).toBeGreaterThan(0);
    expect(screen.queryByText('?')).toBeNull();
    expect(screen.getByText('Native')).toBeInTheDocument();
  });

  it('keeps the honest "?" avatar when nothing names the asset', () => {
    // 527 of the 4,342 type-3 assets on prod carry neither code nor symbol
    // (measured 2026-08-11). The title needs a string, but the avatar must
    // not invent a letter — an "A" from the word "Asset" reads as a ticker.
    mockOk(
      makeAsset({
        id: SAC_CONTRACT,
        asset_type: 3,
        asset_type_name: 'soroban',
        symbol: null,
      })
    );

    renderWithProviders(<AssetDetailPage />, {
      initialEntries: [`/assets/${SAC_CONTRACT}`],
      routePath: '/assets/:id',
    });

    expect(
      screen.getByRole('heading', { level: 1, name: 'Asset' })
    ).toBeInTheDocument();
    expect(screen.getAllByText('?').length).toBeGreaterThan(0);
  });

  it('falls back to the SEP-41 symbol for a Soroban token with no code (0304)', () => {
    mockOk(
      makeAsset({
        id: SAC_CONTRACT,
        asset_type: 3,
        asset_type_name: 'soroban',
        symbol: 'SMOL',
      })
    );

    renderWithProviders(<AssetDetailPage />, {
      initialEntries: [`/assets/${SAC_CONTRACT}`],
      routePath: '/assets/:id',
    });

    expect(
      screen.getByRole('heading', { level: 1, name: 'SMOL' })
    ).toBeInTheDocument();
    expect(screen.getAllByText('S').length).toBeGreaterThan(0);
  });

  it.each([
    ['classic_credit', 'Classic credit'],
    ['soroban', 'Soroban'],
  ])('renders the "%s" type with badge "%s"', (typeName, label) => {
    mockOk(
      makeAsset({
        asset_code: 'USDC',
        asset_type: 1,
        asset_type_name: typeName,
        issuer: ISSUER,
      })
    );

    renderWithProviders(<AssetDetailPage />, {
      initialEntries: [`/assets/USDC-${ISSUER}`],
      routePath: '/assets/:id',
    });

    expect(screen.getByText(label)).toBeInTheDocument();
  });

  it('adds a separate "SAC" tag next to the type badge for a deployed SAC', () => {
    mockOk(
      makeAsset({
        id: `USDC-${ISSUER}`,
        asset_code: 'USDC',
        asset_type: 1,
        asset_type_name: 'classic_credit',
        issuer: ISSUER,
        sac_contract_id: SAC_CONTRACT,
        sac_deployed: true,
      })
    );

    renderWithProviders(<AssetDetailPage />, {
      initialEntries: [`/assets/USDC-${ISSUER}`],
      routePath: '/assets/:id',
    });

    // Two orthogonal axes (ADR 0051): the type badge AND the SAC property tag.
    expect(screen.getByText('Classic credit')).toBeInTheDocument();
    expect(screen.getByText('SAC')).toBeInTheDocument();
    // A deployed SAC is a real contract, so the summary row carries its
    // address, linked (task 0450).
    expect(screen.getByText('SAC contract')).toBeInTheDocument();
    expect(
      screen.getByRole('link', { name: SAC_CONTRACT })
    ).toBeInTheDocument();
  });

  it('shows no "SAC" tag for a reserved (un-deployed) SAC', () => {
    mockOk(
      makeAsset({
        id: `ZK-${ISSUER}`,
        asset_code: 'ZK',
        asset_type: 1,
        asset_type_name: 'classic_credit',
        issuer: ISSUER,
        sac_contract_id: SAC_CONTRACT,
        sac_deployed: false,
      })
    );

    renderWithProviders(<AssetDetailPage />, {
      initialEntries: [`/assets/ZK-${ISSUER}`],
      routePath: '/assets/:id',
    });

    // Reserved SAC: type badge stays, but no SAC property tag (chip label
    // "SAC"). "SAC contract" in the summary row is different exact text.
    expect(screen.getByText('Classic credit')).toBeInTheDocument();
    expect(screen.queryByText('SAC')).not.toBeInTheDocument();
    // The summary row DOES stay, showing the address unlinked with its status.
    // Keeping the oddity visible beats hiding it; making it consistent is
    // task 0452.
    expect(screen.getByText('SAC contract')).toBeInTheDocument();
    expect(
      screen.getByText(/Reserved address — not deployed/)
    ).toBeInTheDocument();
    expect(
      screen.queryByRole('link', { name: SAC_CONTRACT })
    ).not.toBeInTheDocument();
  });

  it('renders the asset name sub-line when present', () => {
    mockOk(
      makeAsset({
        id: 'USDC-XYZ',
        asset_code: 'USDC',
        asset_type: 1,
        asset_type_name: 'classic_credit',
        name: 'USD Coin',
      })
    );

    renderWithProviders(<AssetDetailPage />, {
      initialEntries: [`/assets/USDC-${ISSUER}`],
      routePath: '/assets/:id',
    });

    // Name is rendered in both the header sub-line and the Summary
    // card — assert at least one match.
    expect(screen.getAllByText('USD Coin').length).toBeGreaterThan(0);
  });

  it('renders cleanly when TOML metadata fields are null (partial-metadata tolerance)', () => {
    mockOk(
      makeAsset({
        id: 'EURC-XYZ',
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
      initialEntries: [`/assets/EURC-${ISSUER}`],
      routePath: '/assets/:id',
    });

    expect(
      screen.getByRole('heading', { level: 1, name: 'EURC' })
    ).toBeInTheDocument();
    expect(screen.getByText('Classic credit')).toBeInTheDocument();
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
      initialEntries: [`/assets/USDC-${ISSUER}`],
      routePath: '/assets/:id',
    });

    expect(screen.getByText('Asset not found')).toBeInTheDocument();
  });

  it('renders NotFoundState for a malformed asset id (and skips the fetch)', () => {
    renderWithProviders(<AssetDetailPage />, {
      initialEntries: ['/assets/not-a-valid-id'],
      routePath: '/assets/:id',
    });
    // Pre-validated via `isAssetId`; a non-canonical id is rejected so the
    // detail query is skipped (called with '').
    expect(hookMocks.useAssetDetail).toHaveBeenCalledWith('');
  });
});
