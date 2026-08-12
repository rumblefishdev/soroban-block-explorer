import type { PoolItem, PoolTransactionItem } from '@rumblefish/api-types';
import { screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { renderWithProviders } from '../../test-utils.js';

import { formatPoolAmount, PoolTransactions } from './PoolTransactions.js';

const hookMock = vi.hoisted(() => ({ usePoolTransactions: vi.fn() }));

vi.mock('../../api/index.js', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../api/index.js')>()),
  usePoolTransactions: hookMock.usePoolTransactions,
}));

/** An XLM / USDC pool, in canonical leg order. */
const pool = {
  asset_a: { asset_type_name: 'native', asset_code: null },
  asset_b: { asset_type_name: 'credit_alphanum4', asset_code: 'USDC' },
} as Parameters<typeof formatPoolAmount>[1];

describe('formatPoolAmount', () => {
  it('reads a swap from what entered the pool to what left it', () => {
    // The pool took XLM and gave USDC.
    expect(
      formatPoolAmount({ amount_a: '1000000000', amount_b: '-400000000' }, pool)
    ).toBe('100 XLM → 40 USDC');
  });

  it('orders a swap by direction, not by leg', () => {
    // Same pool, opposite trade: B entered, A left — so USDC reads first.
    expect(
      formatPoolAmount({ amount_a: '-1000000000', amount_b: '400000000' }, pool)
    ).toBe('40 USDC → 100 XLM');
  });

  it('joins both legs of a deposit', () => {
    expect(
      formatPoolAmount(
        { amount_a: '50000000000', amount_b: '20000000000' },
        pool
      )
    ).toBe('5,000 XLM + 2,000 USDC');
  });

  it('joins both legs of a withdrawal', () => {
    expect(
      formatPoolAmount(
        { amount_a: '-50000000000', amount_b: '-20000000000' },
        pool
      )
    ).toBe('5,000 XLM + 2,000 USDC');
  });

  it('renders nothing when the amounts are not known', () => {
    // Rows older than the index — blank, never "0" (no misleading fallbacks).
    expect(
      formatPoolAmount({ amount_a: null, amount_b: null }, pool)
    ).toBeNull();
  });

  it('reads the legs in the pool’s canonical order, not the API’s', () => {
    // Guards the one wiring mistake that would silently swap every row:
    // amount_a belongs to asset_a.
    expect(
      formatPoolAmount({ amount_a: '10000000', amount_b: null }, pool)
    ).toBe('1 XLM');
    expect(
      formatPoolAmount({ amount_a: null, amount_b: '10000000' }, pool)
    ).toBe('1 USDC');
  });

  it('keeps a leg above 2^53 stroops exact', () => {
    // The trailing 1 is the point: as a JSON number this would have rounded
    // to …0000000, which is why the wire format is a string.
    expect(
      formatPoolAmount({ amount_a: '100000000000000001', amount_b: null }, pool)
    ).toBe('10,000,000,000.0000001 XLM');
  });
});

describe('PoolTransactions table', () => {
  const poolItem = {
    asset_a: { asset_type_name: 'native', asset_code: null },
    asset_b: { asset_type_name: 'credit_alphanum4', asset_code: 'USDC' },
  } as PoolItem;

  const row = {
    hash: 'a'.repeat(64),
    ledger_sequence: 63_904_097,
    source_account: 'G'.repeat(56),
    fee_charged: 100,
    successful: true,
    operation_count: 1,
    has_soroban: false,
    operation_types: ['PATH_PAYMENT_STRICT_SEND'],
    created_at: '2026-08-11T14:26:36Z',
    amount_a: '1000000000',
    amount_b: '-400000000',
  } as PoolTransactionItem;

  it('shows the Amount column and the row’s swap', () => {
    hookMock.usePoolTransactions.mockReturnValue({
      data: { data: [row], page: { limit: 20 } },
      isLoading: false,
      isPlaceholderData: false,
      isError: false,
      error: null,
      refetch: vi.fn(),
    });

    renderWithProviders(<PoolTransactions poolId="LBSU" pool={poolItem} />);

    expect(screen.getByText('Amount')).toBeInTheDocument();
    expect(screen.getByText('100 XLM → 40 USDC')).toBeInTheDocument();
  });
});
