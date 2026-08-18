import type { PoolActivityItem, PoolItem } from '@rumblefish/api-types';
import { screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { renderWithProviders } from '../../test-utils.js';

import {
  activityRowKey,
  formatPoolAmount,
  PoolActivity,
} from './PoolActivity.js';

const hookMock = vi.hoisted(() => ({ usePoolActivity: vi.fn() }));

vi.mock('../../api/index.js', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../api/index.js')>()),
  usePoolActivity: hookMock.usePoolActivity,
}));

/** An XLM / USDC pool, in canonical leg order. */
const pool = {
  asset_a: { asset_type_name: 'native', asset_code: null },
  asset_b: { asset_type_name: 'credit_alphanum4', asset_code: 'USDC' },
} as Parameters<typeof formatPoolAmount>[1];

describe('formatPoolAmount', () => {
  it('reads a swap from what entered the pool to what left it', () => {
    expect(
      formatPoolAmount({ amount_a: '1200000000', amount_b: '-5000000' }, pool)
    ).toBe('120 XLM → 0.5 USDC');
  });

  it('orders a swap by direction, not by leg', () => {
    expect(
      formatPoolAmount({ amount_a: '-1200000000', amount_b: '5000000' }, pool)
    ).toBe('0.5 USDC → 120 XLM');
  });

  it('joins both legs of a deposit', () => {
    expect(
      formatPoolAmount({ amount_a: '1200000000', amount_b: '5000000' }, pool)
    ).toBe('120 XLM + 0.5 USDC');
  });

  it('renders nothing when neither leg is known', () => {
    expect(
      formatPoolAmount({ amount_a: null, amount_b: null }, pool)
    ).toBeNull();
  });

  it('keeps a leg above 2^53 stroops exact', () => {
    expect(
      formatPoolAmount({ amount_a: '90071992547409910', amount_b: null }, pool)
    ).toBe('9,007,199,254.740991 XLM');
  });
});

describe('activityRowKey', () => {
  /** The hash alone is NOT unique: one transaction can run several
   *  operations against the same pool, and each is its own row. */
  it('separates two operations of one transaction', () => {
    const hash = 'a'.repeat(64);
    const first = { transaction_hash: hash, application_order: 1 };
    const second = { transaction_hash: hash, application_order: 2 };
    expect(activityRowKey(first as PoolActivityItem)).not.toBe(
      activityRowKey(second as PoolActivityItem)
    );
  });
});

describe('PoolActivity table', () => {
  const poolItem = {
    asset_a: { asset_type_name: 'native', asset_code: null },
    asset_b: { asset_type_name: 'credit_alphanum4', asset_code: 'USDC' },
  } as PoolItem;

  const makeRow = (over: Partial<PoolActivityItem> = {}) =>
    ({
      transaction_hash: 'a'.repeat(64),
      ledger_sequence: 63_904_097,
      application_order: 1,
      event: 'trade',
      amount_a: '1200000000',
      amount_b: '-5000000',
      source_account: 'G'.repeat(56),
      created_at: '2026-08-11T14:26:36Z',
      ...over,
    } as PoolActivityItem);

  function mockRows(rows: PoolActivityItem[]) {
    hookMock.usePoolActivity.mockReturnValue({
      data: { data: rows, page: { limit: 20 } },
      isLoading: false,
      isPlaceholderData: false,
      isError: false,
      error: null,
      refetch: vi.fn(),
    });
  }

  it('renders one row per operation, each with its own event and figure', () => {
    mockRows([
      makeRow({ application_order: 1, event: 'deposit', amount_b: '5000000' }),
      makeRow({ application_order: 2, event: 'trade' }),
    ]);
    renderWithProviders(<PoolActivity poolId="LPOOL" pool={poolItem} />);

    // Scoped to the table on purpose: the filter control offers the same three
    // words, so an unscoped query matches the button as well as the chip.
    const table = within(screen.getByRole('table'));

    // The bundle that made the per-transaction chip lie: one deposit and one
    // trade in the same transaction, now labelled correctly one row each.
    expect(table.getByText('Deposit')).toBeInTheDocument();
    expect(table.getByText('Trade')).toBeInTheDocument();
    expect(table.getByText('120 XLM + 0.5 USDC')).toBeInTheDocument();
    expect(table.getByText('120 XLM → 0.5 USDC')).toBeInTheDocument();
  });

  it('links a row to its own operation anchor, not just the transaction', () => {
    mockRows([makeRow({ application_order: 7 })]);
    renderWithProviders(<PoolActivity poolId="LPOOL" pool={poolItem} />);

    const link = screen
      .getAllByRole('link')
      .find((a) => a.getAttribute('href')?.includes('#op-'));
    expect(link?.getAttribute('href')).toBe(
      `/transactions/${'a'.repeat(64)}#op-7`
    );
  });

  it('renders no figure for a row whose legs did not both land', () => {
    mockRows([makeRow({ event: null, amount_a: null, amount_b: null })]);
    renderWithProviders(<PoolActivity poolId="LPOOL" pool={poolItem} />);

    // Not a zero and not a dash — "not known" is not "nothing moved".
    expect(screen.queryByText('0 XLM')).not.toBeInTheDocument();
    expect(screen.queryByText('—')).not.toBeInTheDocument();
  });

  it('says the pool is empty only when no filter is narrowing it', () => {
    mockRows([]);
    renderWithProviders(<PoolActivity poolId="LPOOL" pool={poolItem} />);
    expect(screen.getByText('No activity yet')).toBeInTheDocument();
  });
});
