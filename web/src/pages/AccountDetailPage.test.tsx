import type {
  AccountBalance,
  AccountDetailResponse,
} from '@rumblefish/api-types';
import { screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { renderWithProviders } from '../test-utils.js';

import AccountDetailPage from './AccountDetailPage.js';

const hookMocks = vi.hoisted(() => ({
  useAccountDetail: vi.fn(),
  useAccountTransactions: vi.fn(),
}));

vi.mock('../api/index.js', async () => ({
  useAccountDetail: hookMocks.useAccountDetail,
  useAccountTransactions: hookMocks.useAccountTransactions,
  // Pure pagination helper — keep the real implementation so the
  // transactions section renders instead of hitting its error boundary.
  usePagedRows: (
    await vi.importActual<typeof import('../api/usePagedRows.js')>(
      '../api/usePagedRows.js'
    )
  ).usePagedRows,
}));

const VALID_ACCOUNT =
  'GDQP2KPQGKIHYJGXNUIYOMHARUARCA7DJT5FO2FFOOUJ3K4MOMNGEE36';
const USDC_ISSUER = 'GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN';

const NATIVE_BALANCE: AccountBalance = {
  asset_type_name: 'native',
  asset_code: null,
  asset_issuer: null,
  // RAW Int128 (task 0331) — 500 XLM scaled by decimals=7.
  balance: '5000000000',
  decimals: 7,
  last_updated_ledger: 100,
  type: 0,
  sac_deployed: false,
};
const USDC_BALANCE: AccountBalance = {
  asset_type_name: 'credit_alphanum4',
  asset_code: 'USDC',
  asset_issuer: USDC_ISSUER,
  // RAW Int128 (task 0331) — 1250.5 USDC scaled by decimals=7.
  balance: '12505000000',
  decimals: 7,
  last_updated_ledger: 100,
  type: 1,
  sac_deployed: false,
};

const SAMPLE: AccountDetailResponse = {
  account_id: VALID_ACCOUNT,
  balances: [NATIVE_BALANCE, USDC_BALANCE],
  first_seen_ledger: 50,
  last_seen_ledger: 200,
  sequence_number: 99_123_456,
  deleted: false,
};

const DELETED_SAMPLE: AccountDetailResponse = {
  ...SAMPLE,
  deleted: true,
};

function mockDetail(value: unknown): void {
  hookMocks.useAccountDetail.mockReturnValue(value);
}

beforeEach(() => {
  hookMocks.useAccountDetail.mockReset();
  // Default the transactions hook so the test never blows up when the
  // section renders — page-level assertions don't care about tx rows.
  hookMocks.useAccountTransactions.mockReturnValue({
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

describe('AccountDetailPage', () => {
  it('renders NotFoundState for a malformed account id (and skips the fetch)', () => {
    renderWithProviders(<AccountDetailPage />, {
      initialEntries: ['/accounts/not-a-strkey'],
      routePath: '/accounts/:accountId',
    });

    expect(screen.getByText('Account not found')).toBeInTheDocument();
    // The hook is still called with the empty fallback (guard string),
    // never with the malformed input.
    expect(hookMocks.useAccountDetail).toHaveBeenCalledWith('');
  });

  it('renders the summary + balances for a valid account', () => {
    mockDetail({
      data: SAMPLE,
      isLoading: false,
      isError: false,
      error: null,
      refetch: vi.fn(),
    });

    renderWithProviders(<AccountDetailPage />, {
      initialEntries: [`/accounts/${VALID_ACCOUNT}`],
      routePath: '/accounts/:accountId',
    });

    expect(
      screen.getByRole('heading', { level: 1, name: 'Account' })
    ).toBeInTheDocument();
    // Full account id rendered in at least one place (header sub-line +
    // copy-link inside the Summary card both repeat it).
    expect(screen.getAllByText(VALID_ACCOUNT).length).toBeGreaterThan(0);
    // Native balance always shows the "Stellar Lumens" name.
    expect(screen.getByText('Stellar Lumens')).toBeInTheDocument();
    // RAW balance is scaled by decimals for display (task 0331): native
    // `5000000000` / 7 → `500.00`; classic `12505000000` / 7 → `1,250.50`.
    expect(screen.getByText('500.00')).toBeInTheDocument();
    expect(screen.getByText('1,250.50')).toBeInTheDocument();
    // Classic credit balance shows the code as the row name.
    expect(screen.getAllByText('USDC').length).toBeGreaterThan(0);
  });

  it('counts the assets and, separately, how many carry value', () => {
    // A card titled "Balances" listing thousands of zeros argues with its own
    // contents — the zeros are real holdings (issue #377), so the card is
    // "Assets" and the two numbers are stated apart.
    mockDetail({
      data: {
        ...SAMPLE,
        balances: [NATIVE_BALANCE, { ...USDC_BALANCE, balance: '0' }],
      },
      isLoading: false,
      isError: false,
      error: null,
      refetch: vi.fn(),
    });

    renderWithProviders(<AccountDetailPage />, {
      initialEntries: [`/accounts/${VALID_ACCOUNT}`],
      routePath: '/accounts/:accountId',
    });

    expect(screen.getByText('Assets')).toBeInTheDocument();
    expect(screen.getByText('2 assets · 1 with a balance')).toBeInTheDocument();
  });

  it('drops the second clause when every asset carries value', () => {
    // Restating the same number twice reads as bureaucracy, not information.
    mockDetail({
      data: SAMPLE,
      isLoading: false,
      isError: false,
      error: null,
      refetch: vi.fn(),
    });

    renderWithProviders(<AccountDetailPage />, {
      initialEntries: [`/accounts/${VALID_ACCOUNT}`],
      routePath: '/accounts/:accountId',
    });

    expect(screen.getByText('2 assets')).toBeInTheDocument();
  });

  it('renders the assets in the order the API returned them', () => {
    // The server pins native, then funded before empty, then size, then
    // recency. Re-sorting here would put a page boundary somewhere other than
    // where the server put it.
    const zeroA = { ...USDC_BALANCE, asset_code: 'ZZZA', balance: '0' };
    const zeroB = { ...USDC_BALANCE, asset_code: 'AAAB', balance: '0' };
    mockDetail({
      data: { ...SAMPLE, balances: [NATIVE_BALANCE, zeroA, zeroB] },
      isLoading: false,
      isError: false,
      error: null,
      refetch: vi.fn(),
    });

    renderWithProviders(<AccountDetailPage />, {
      initialEntries: [`/accounts/${VALID_ACCOUNT}`],
      routePath: '/accounts/:accountId',
    });

    // Compared by position in the rendered text: each code also appears as the
    // ticker under its amount, so counting elements would double them.
    const text = document.body.textContent ?? '';
    expect(text.indexOf('Stellar Lumens')).toBeLessThan(text.indexOf('ZZZA'));
    expect(text.indexOf('ZZZA')).toBeLessThan(text.indexOf('AAAB'));
  });

  it('tags a classic balance whose SAC is deployed, and leaves the type chip alone', () => {
    // Two orthogonal axes (ADR 0051): the type chip stays "Classic credit",
    // the SAC facet is a SECOND tag. Before this field existed the page
    // inferred the facet from the issuer address starting with `C`, which
    // `asset_issuer` never is — so the tag could not render even once.
    mockDetail({
      data: {
        ...SAMPLE,
        balances: [{ ...USDC_BALANCE, sac_deployed: true }],
      },
      isLoading: false,
      isError: false,
      error: null,
      refetch: vi.fn(),
    });

    renderWithProviders(<AccountDetailPage />, {
      initialEntries: [`/accounts/${VALID_ACCOUNT}`],
      routePath: '/accounts/:accountId',
    });

    expect(screen.getByText('SAC')).toBeInTheDocument();
    expect(screen.getByText('Classic credit')).toBeInTheDocument();
  });

  it('tags native XLM too, because the assets pages do', () => {
    // XLM really does have a deployed SAC, and `/assets` (list AND detail)
    // renders the tag ungated by asset type. The account page must not be the
    // one view that hides it.
    mockDetail({
      data: {
        ...SAMPLE,
        balances: [{ ...NATIVE_BALANCE, sac_deployed: true }],
      },
      isLoading: false,
      isError: false,
      error: null,
      refetch: vi.fn(),
    });

    renderWithProviders(<AccountDetailPage />, {
      initialEntries: [`/accounts/${VALID_ACCOUNT}`],
      routePath: '/accounts/:accountId',
    });

    expect(screen.getByText('Stellar Lumens')).toBeInTheDocument();
    expect(screen.getByText('SAC')).toBeInTheDocument();
  });

  it('shows no SAC tag for a classic balance without a deployed one', () => {
    mockDetail({
      data: { ...SAMPLE, balances: [USDC_BALANCE] },
      isLoading: false,
      isError: false,
      error: null,
      refetch: vi.fn(),
    });

    renderWithProviders(<AccountDetailPage />, {
      initialEntries: [`/accounts/${VALID_ACCOUNT}`],
      routePath: '/accounts/:accountId',
    });

    // A reserved-but-undeployed SAC is an address, not a contract — no tag.
    expect(screen.queryByText('SAC')).not.toBeInTheDocument();
    expect(screen.getByText('Classic credit')).toBeInTheDocument();
  });

  it('links a classic-credit balance to its /assets/:id detail page', () => {
    mockDetail({
      data: SAMPLE,
      isLoading: false,
      isError: false,
      error: null,
      refetch: vi.fn(),
    });

    renderWithProviders(<AccountDetailPage />, {
      initialEntries: [`/accounts/${VALID_ACCOUNT}`],
      routePath: '/accounts/:accountId',
    });

    const usdcLink = screen.getByRole('link', { name: 'USDC' });
    expect(usdcLink).toHaveAttribute(
      'href',
      `/assets/${encodeURIComponent(`USDC-${USDC_ISSUER}`)}`
    );
  });

  it('shows a Deleted badge for a merged account', () => {
    mockDetail({
      data: DELETED_SAMPLE,
      isLoading: false,
      isError: false,
      error: null,
      refetch: vi.fn(),
    });

    renderWithProviders(<AccountDetailPage />, {
      initialEntries: [`/accounts/${VALID_ACCOUNT}`],
      routePath: '/accounts/:accountId',
    });

    expect(screen.getByText('Deleted')).toBeInTheDocument();
  });

  it('omits the Deleted badge for a live account', () => {
    mockDetail({
      data: SAMPLE,
      isLoading: false,
      isError: false,
      error: null,
      refetch: vi.fn(),
    });

    renderWithProviders(<AccountDetailPage />, {
      initialEntries: [`/accounts/${VALID_ACCOUNT}`],
      routePath: '/accounts/:accountId',
    });

    expect(screen.queryByText('Deleted')).not.toBeInTheDocument();
  });

  it('renders NotFoundState when the detail query 404s', () => {
    mockDetail({
      data: undefined,
      isLoading: false,
      isError: true,
      // `classifyError` reads `.status` to detect missing-resource.
      error: Object.assign(new Error('not found'), { status: 404 }),
      refetch: vi.fn(),
    });

    renderWithProviders(<AccountDetailPage />, {
      initialEntries: [`/accounts/${VALID_ACCOUNT}`],
      routePath: '/accounts/:accountId',
    });

    expect(screen.getByText('Account not found')).toBeInTheDocument();
  });

  it('renders GenericErrorState for a non-404 fetch error and isolates it from the transactions section', () => {
    mockDetail({
      data: undefined,
      isLoading: false,
      isError: true,
      error: Object.assign(new Error('boom'), { status: 500 }),
      refetch: vi.fn(),
    });

    renderWithProviders(<AccountDetailPage />, {
      initialEntries: [`/accounts/${VALID_ACCOUNT}`],
      routePath: '/accounts/:accountId',
    });

    // Generic error state surfaces the retry CTA.
    expect(
      screen.getByRole('button', { name: /try again/i })
    ).toBeInTheDocument();
    // Header still rendered — the parent error doesn't tear down the page.
    expect(
      within(screen.getByRole('heading', { level: 1 })).getByText('Account')
    ).toBeInTheDocument();
    // Transactions section is intentionally hidden on detail error.
    expect(screen.queryByText(/recent transactions/i)).not.toBeInTheDocument();
  });
});
