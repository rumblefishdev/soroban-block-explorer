import type {
  ContractDetailResponse,
  SourceVerificationResponse,
  VerificationStatus,
} from '@rumblefish/api-types';
import { screen, within } from '@testing-library/react';
import { userEvent } from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { renderWithProviders } from '../../test-utils.js';

import ContractDetailPage from '../ContractDetailPage.js';

const CONTRACT = 'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75';
const ISSUER = 'GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN';

const hookMocks = vi.hoisted(() => ({
  useContractDetail: vi.fn(),
  useContractVerification: vi.fn(),
}));

vi.mock('../../api/index.js', () => ({
  useContractDetail: hookMocks.useContractDetail,
  useContractVerification: hookMocks.useContractVerification,
}));

// The tabbed sub-sections carry their own queries and error boundaries; the
// header face is what this suite covers, so stub them out.
vi.mock('../contracts/ContractInterface.js', () => ({
  ContractInterface: () => null,
}));
vi.mock('../contracts/ContractCode.js', () => ({ ContractCode: () => null }));
vi.mock('../contracts/ContractInvocations.js', () => ({
  ContractInvocations: () => null,
}));
vi.mock('../contracts/ContractEvents.js', () => ({
  ContractEvents: () => null,
}));
vi.mock('../contracts/VerifiedCode.js', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../contracts/VerifiedCode.js')>()),
  VerifiedCode: () => <div>verified code tab body</div>,
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
  hookMocks.useContractVerification.mockReset();
  hookMocks.useContractVerification.mockReturnValue({
    data: undefined,
    isPending: false,
  });
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

// Task 0548 — a CAP-85 fleet member runs code set by another contract. The
// header must say so instead of looking like a contract with its own code.
describe('ContractDetailPage externally managed executable (task 0548)', () => {
  const OWNER = 'CBGVOLVXQ4XJZC6F4CQKWNKA4VL36VYVVSMV2RJD3QFYVI6RCJFJDDOF';

  it('shows the Externally managed chip when the contract has an owner', () => {
    mockOk(
      makeContract({
        wasm_hash: 'bb'.repeat(32),
        executable_owner: OWNER,
        executable_tag: 'fleet-v2',
      })
    );
    renderPage();

    expect(screen.getByText('Externally managed')).toBeInTheDocument();
  });

  it('shows no such chip for a contract that carries its own code', () => {
    mockOk(makeContract({ wasm_hash: 'aa'.repeat(32) }));
    renderPage();

    expect(screen.queryByText('Externally managed')).toBeNull();
  });
});

// Task 0639 — a verified result adds the Verified Code tab and a header chip;
// every other answer (mismatched, disputed, none, an error) shows nothing.
describe('ContractDetailPage source verification (task 0639)', () => {
  const verification = (
    status: VerificationStatus
  ): SourceVerificationResponse => ({
    contract_id: CONTRACT,
    wasm_hash: 'aa'.repeat(32),
    status,
    checked_at: '2026-10-14T13:30:48.961Z',
    verifiers: [
      {
        name: 'SCVS Staging Verifier',
        url: 'https://results.example',
        status: 'verified',
      },
    ],
    source: null,
    build: null,
  });
  const answer = (
    data: SourceVerificationResponse | undefined,
    isPending = false
  ) => hookMocks.useContractVerification.mockReturnValue({ data, isPending });
  const tabs = () => screen.getByRole('tablist', { name: 'Contract sections' });

  it('renames the Code tab to Decompilation', () => {
    mockOk(makeContract({ wasm_hash: 'aa'.repeat(32) }));
    renderPage();

    expect(within(tabs()).getByText('Decompilation')).toBeInTheDocument();
    expect(within(tabs()).queryByText('Code')).toBeNull();
  });

  it.each<VerificationStatus>(['verified', 'verified_private'])(
    'shows the chip and the tab for %s',
    (status) => {
      mockOk(makeContract({ wasm_hash: 'aa'.repeat(32) }));
      answer(verification(status));
      renderPage();

      expect(screen.getByText('Verified source')).toBeInTheDocument();
      expect(within(tabs()).getByText('Verified Code')).toBeInTheDocument();
    }
  );

  it.each<VerificationStatus>([
    'mismatched',
    'disputed',
    'unverified',
    'none',
    'unavailable',
  ])('shows nothing for %s', (status) => {
    mockOk(makeContract({ wasm_hash: 'aa'.repeat(32) }));
    answer(verification(status));
    renderPage();

    expect(screen.queryByText('Verified source')).toBeNull();
    expect(within(tabs()).queryByText('Verified Code')).toBeNull();
  });

  it('shows nothing when there is no answer (404 or an error)', () => {
    mockOk(makeContract({ wasm_hash: 'aa'.repeat(32) }));
    answer(undefined);
    renderPage();

    expect(screen.queryByText('Verified source')).toBeNull();
    expect(within(tabs()).queryByText('Verified Code')).toBeNull();
  });

  it('does not ask about a contract without WASM', () => {
    mockOk(makeContract({ wasm_hash: null }));
    renderPage();

    expect(hookMocks.useContractVerification).toHaveBeenCalledWith(
      CONTRACT,
      false
    );
  });

  it('keeps a deep link on the tab while the answer loads', () => {
    mockOk(makeContract({ wasm_hash: 'aa'.repeat(32) }));
    answer(undefined, true);
    renderWithProviders(<ContractDetailPage />, {
      initialEntries: [`/contracts/${CONTRACT}?tab=verified`],
      routePath: '/contracts/:contractId',
    });

    expect(within(tabs()).getByText('Verified Code')).toBeInTheDocument();
  });

  it('shows the error with Try again on a deep link when the answer fails', () => {
    mockOk(makeContract({ wasm_hash: 'aa'.repeat(32) }));
    hookMocks.useContractVerification.mockReturnValue({
      data: undefined,
      isPending: false,
      isError: true,
      error: Object.assign(new Error('boom'), { status: 500 }),
      refetch: vi.fn(),
    });
    renderWithProviders(<ContractDetailPage />, {
      initialEntries: [`/contracts/${CONTRACT}?tab=verified`],
      routePath: '/contracts/:contractId',
    });

    expect(within(tabs()).getByText('Verified Code')).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Try again' })
    ).toBeInTheDocument();
  });

  it('Try again asks for the verification again', async () => {
    mockOk(makeContract({ wasm_hash: 'aa'.repeat(32) }));
    const refetch = vi.fn();
    hookMocks.useContractVerification.mockReturnValue({
      data: undefined,
      isPending: false,
      isError: true,
      error: Object.assign(new Error('boom'), { status: 500 }),
      refetch,
    });
    renderWithProviders(<ContractDetailPage />, {
      initialEntries: [`/contracts/${CONTRACT}?tab=verified`],
      routePath: '/contracts/:contractId',
    });

    await userEvent.click(screen.getByRole('button', { name: 'Try again' }));
    expect(refetch).toHaveBeenCalledTimes(1);
  });

  it('the header chip is a plain label: a click neither opens a tab nor scrolls', async () => {
    Element.prototype.scrollIntoView = vi.fn();
    mockOk(makeContract({ wasm_hash: 'aa'.repeat(32) }));
    answer(verification('verified'));
    renderPage();

    expect(
      screen.queryByRole('button', { name: 'Verified source' })
    ).toBeNull();
    await userEvent.click(screen.getByText('Verified source'));
    expect(
      within(tabs()).getByRole('tab', { name: /Verified Code/ })
    ).toHaveAttribute('aria-selected', 'false');
    expect(Element.prototype.scrollIntoView).not.toHaveBeenCalled();
  });

  it('an old ?tab=code link still opens the renamed Decompilation tab', () => {
    mockOk(makeContract({ wasm_hash: 'aa'.repeat(32) }));
    renderWithProviders(<ContractDetailPage />, {
      initialEntries: [`/contracts/${CONTRACT}?tab=code`],
      routePath: '/contracts/:contractId',
    });

    expect(
      within(tabs()).getByRole('tab', { name: /Decompilation/ })
    ).toHaveAttribute('aria-selected', 'true');
  });

  it('a 404 on a deep link (no contract or WASM): back to Interface, no error', () => {
    mockOk(makeContract({ wasm_hash: 'aa'.repeat(32) }));
    hookMocks.useContractVerification.mockReturnValue({
      data: undefined,
      isPending: false,
      isError: true,
      error: Object.assign(new Error('not found'), { status: 404 }),
      refetch: vi.fn(),
    });
    renderWithProviders(<ContractDetailPage />, {
      initialEntries: [`/contracts/${CONTRACT}?tab=verified`],
      routePath: '/contracts/:contractId',
    });

    expect(within(tabs()).queryByText('Verified Code')).toBeNull();
    expect(screen.queryByRole('button', { name: 'Try again' })).toBeNull();
  });

  it('names the verifiers in the chip tooltip', async () => {
    mockOk(makeContract({ wasm_hash: 'aa'.repeat(32) }));
    answer(verification('verified'));
    renderPage();

    await userEvent.hover(screen.getByText('Verified source'));
    const tip = await screen.findByRole('tooltip');
    expect(tip).toHaveTextContent('Rebuilt from its source by:');
    expect(within(tip).getByText('SCVS Staging Verifier')).toBeInTheDocument();
    expect(tip).toHaveTextContent('Not an audit.');
  });

  it('opens the tab body from a deep link once verified', () => {
    mockOk(makeContract({ wasm_hash: 'aa'.repeat(32) }));
    answer(verification('verified'));
    renderWithProviders(<ContractDetailPage />, {
      initialEntries: [`/contracts/${CONTRACT}?tab=verified`],
      routePath: '/contracts/:contractId',
    });

    expect(screen.getByText('verified code tab body')).toBeInTheDocument();
  });
});

describe('ContractDetailPage muxed address (task 0649)', () => {
  it('sends a W-address URL to the page of the contract it stands for', () => {
    const c = 'CDL74RF5BLYR2YBLCCI7F5FB6TPSCLKEJUBSD2RSVWZ4YHF3VMFAIGWA';
    const w =
      'WDL74RF5BLYR2YBLCCI7F5FB6TPSCLKEJUBSD2RSVWZ4YHF3VMFAIAAAAAAAAAAAFLR56';
    mockOk(makeContract({ contract_id: c }));
    renderWithProviders(<ContractDetailPage />, {
      initialEntries: [`/contracts/${w}?tab=events`],
      routePath: '/contracts/:contractId',
    });
    expect(hookMocks.useContractDetail).toHaveBeenLastCalledWith(c);
    expect(hookMocks.useContractDetail).not.toHaveBeenCalledWith(w);
  });
});
