import type { ContractDetailResponse } from '@rumblefish/api-types';
import { screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { renderWithProviders } from '../test-utils.js';

import ContractDetailPage from './ContractDetailPage.js';

const CONTRACT = 'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75';
const ISSUER = 'GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN';

const hookMocks = vi.hoisted(() => ({
  useContractDetail: vi.fn(),
}));

vi.mock('../api/index.js', () => ({
  useContractDetail: hookMocks.useContractDetail,
}));

// The tabbed sub-sections carry their own queries and error boundaries; the
// header face is what this suite covers, so stub them out.
vi.mock('./contracts/ContractInterface.js', () => ({
  ContractInterface: () => null,
}));
vi.mock('./contracts/ContractCode.js', () => ({ ContractCode: () => null }));
vi.mock('./contracts/ContractInvocations.js', () => ({
  ContractInvocations: () => null,
}));
vi.mock('./contracts/ContractEvents.js', () => ({
  ContractEvents: () => null,
}));

function makeContract(
  overrides: Partial<ContractDetailResponse> = {}
): ContractDetailResponse {
  return {
    contract_id: CONTRACT,
    contract_type: 0,
    contract_type_name: 'token',
    is_sac: false,
    sac_asset: null,
    deployer: null,
    deployed_at_ledger: 50_463_389,
    wasm_hash: null,
    stats: {
      recent_events: 0,
      recent_invocations: 0,
      recent_unique_callers: 0,
      stats_window: '7 days',
    },
    ...overrides,
  };
}

function mockOk(contract: ContractDetailResponse): void {
  hookMocks.useContractDetail.mockReturnValue({
    data: contract,
    isLoading: false,
    isError: false,
    error: null,
    refetch: vi.fn(),
  });
}

function renderPage() {
  renderWithProviders(<ContractDetailPage />, {
    initialEntries: [`/contracts/${CONTRACT}`],
    routePath: '/contracts/:contractId',
  });
}

beforeEach(() => {
  hookMocks.useContractDetail.mockReset();
});

afterEach(() => {
  vi.clearAllMocks();
});

// Render-level coverage for the header face (task 0472). The helper's unit
// tests assert the decision; these assert the page actually turns it into a
// single REAL anchor — the a11y review found the first version rendered a
// keyboard-dead role="button" nested inside the link, which no helper test
// could have caught.
describe('ContractDetailPage header face (task 0472)', () => {
  it('renders one anchor naming the mirrored asset for a SAC', () => {
    mockOk(
      makeContract({
        is_sac: true,
        sac_asset: { asset_code: 'USDC', issuer: ISSUER },
      })
    );
    renderPage();

    // The visible text IS the accessible name (WCAG 2.5.3) — the issuer
    // tooltip is a description, not a replacement.
    const link = screen.getByRole('link', {
      name: 'Stellar Asset Contract · USDC',
    });
    expect(link).toHaveAttribute('href', `/assets/USDC-${ISSUER}`);
    // No nested interactive control: the chip inside must not be a button.
    expect(link.querySelector('[role="button"]')).toBeNull();
  });

  it('links a Fungible contract to its own asset page', () => {
    mockOk(makeContract({ contract_type: 3, contract_type_name: 'fungible' }));
    renderPage();

    expect(screen.getByRole('link', { name: 'Fungible' })).toHaveAttribute(
      'href',
      `/assets/${CONTRACT}`
    );
  });

  it('links an NFT contract to its filtered collection view', () => {
    mockOk(makeContract({ contract_type: 2, contract_type_name: 'nft' }));
    renderPage();

    expect(screen.getByRole('link', { name: 'NFT' })).toHaveAttribute(
      'href',
      `/nfts?contract=${CONTRACT}`
    );
  });

  it('renders Other as a plain unlinked chip', () => {
    mockOk(makeContract({ contract_type: 1, contract_type_name: 'other' }));
    renderPage();

    expect(screen.getByText('Other')).toBeInTheDocument();
    expect(screen.queryByRole('link', { name: 'Other' })).toBeNull();
  });

  it('degrades a SAC with an unresolvable facet to a bare unlinked chip', () => {
    mockOk(makeContract({ is_sac: true, sac_asset: null }));
    renderPage();

    expect(screen.getByText('Stellar Asset Contract')).toBeInTheDocument();
    expect(
      screen.queryByRole('link', { name: /Stellar Asset Contract/ })
    ).toBeNull();
  });
});
