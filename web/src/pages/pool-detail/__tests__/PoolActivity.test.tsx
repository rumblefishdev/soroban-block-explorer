import type { PoolActivityItem, PoolItem } from '@rumblefish/api-types';
import { screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { renderWithProviders } from '../../../test-utils.js';

import {
  activityRowKey,
  formatPoolAmount,
  PoolActivity,
  poolAmountLegs,
  tradeRate,
} from '../PoolActivity.js';

const hookMock = vi.hoisted(() => ({ usePoolActivity: vi.fn() }));

vi.mock('../../../api/index.js', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../../api/index.js')>()),
  usePoolActivity: hookMock.usePoolActivity,
}));

/** An XLM / USDC pool, in registration order. */
const pool = {
  legs: [
    { asset_type_name: 'native', asset_code: null, decimals: 7 },
    { asset_type_name: 'classic_credit', asset_code: 'USDC', decimals: 7 },
  ],
} as Parameters<typeof formatPoolAmount>[1];

describe('formatPoolAmount', () => {
  it('reads a swap from what entered the pool to what left it', () => {
    expect(
      formatPoolAmount({ amounts: ['1200000000', '-5000000'] }, pool)
    ).toBe('120 XLM → 0.5 USDC');
  });

  it('orders a swap by direction, not by leg', () => {
    expect(
      formatPoolAmount({ amounts: ['-1200000000', '5000000'] }, pool)
    ).toBe('0.5 USDC → 120 XLM');
  });

  it('joins both legs of a deposit', () => {
    expect(formatPoolAmount({ amounts: ['1200000000', '5000000'] }, pool)).toBe(
      '120 XLM + 0.5 USDC'
    );
  });

  it('renders nothing when neither leg is known', () => {
    expect(formatPoolAmount({ amounts: [null, null] }, pool)).toBeNull();
  });

  it('keeps a leg above 2^53 stroops exact', () => {
    expect(
      formatPoolAmount({ amounts: ['90071992547409910', null] }, pool)
    ).toBe('9,007,199,254.740991 XLM');
  });
});

/** A soroban stable pool: XLM, a token publishing 18 decimals, a token
 *  publishing 6 and one publishing none. */
const sorobanPool = {
  legs: [
    { asset_type_name: 'native', asset_code: null, decimals: 7 },
    { asset_type_name: 'soroban', symbol: 'WETH', decimals: 18 },
    { asset_type_name: 'soroban', symbol: 'USDx', decimals: 6 },
    { asset_type_name: 'soroban', symbol: 'ODD', decimals: null },
  ],
} as Parameters<typeof formatPoolAmount>[1];

describe('formatPoolAmount on a soroban pool', () => {
  it('scales each leg by its own decimals', () => {
    expect(
      formatPoolAmount(
        { amounts: ['1500000000000000000', '-2500000', null, null] },
        { legs: [sorobanPool.legs[1], sorobanPool.legs[2]] }
      )
    ).toBe('1.5 WETH → 2.5 USDx');
  });

  it('reads a swap on a three-token pool by the two legs it names', () => {
    const parts = poolAmountLegs(
      { amounts: ['10000000', null, '-1000000', null] },
      sorobanPool
    );
    expect(parts?.swap).toBe(true);
    expect(
      formatPoolAmount(
        { amounts: ['10000000', null, '-1000000', null] },
        sorobanPool
      )
    ).toBe('1 XLM → 1 USDx');
    // The rate is of scaled amounts; in raw units it would be off by 10.
    expect(tradeRate(parts)).toBe('1 USDx/XLM');
  });

  it('leaves out a leg whose token publishes no decimals', () => {
    expect(
      formatPoolAmount(
        { amounts: ['10000000', null, null, '123456789'] },
        sorobanPool
      )
    ).toBe('1 XLM');
  });
});

describe('formatPoolAmount with a zero leg', () => {
  /** A zero is a known amount, not an absent one: a one-sided deposit into
   *  a stable pool and a swap that paid out nothing both say so. */
  it('shows a leg that moved exactly zero', () => {
    expect(formatPoolAmount({ amounts: ['50000000', '0'] }, pool)).toBe(
      '5 XLM + 0 USDC'
    );
  });
});

describe('tradeRate', () => {
  /** Quoted as out-per-in, the way stellar.expert does — the real fbdfc7ec
   *  trade reads `at 3,063 KALE/XLM` there and must read the same here. */
  it('quotes a swap as out per in, 4 significant figures', () => {
    const parts = poolAmountLegs({ amounts: ['1253398', '-3839199963'] }, pool);
    expect(tradeRate(parts)).toBe('3,063 USDC/XLM');
  });

  it('keeps sub-one rates readable instead of rounding them to zero', () => {
    const parts = poolAmountLegs({ amounts: ['-62441', '192417893'] }, pool);
    expect(tradeRate(parts)).toBe('0.0003245 XLM/USDC');
  });

  it('has no rate for a deposit and no rate against a zero leg', () => {
    expect(
      tradeRate(poolAmountLegs({ amounts: ['1200000000', '5000000'] }, pool))
    ).toBeNull();
    expect(
      tradeRate(poolAmountLegs({ amounts: ['0', '-5000000'] }, pool))
    ).toBeNull();
  });
});

describe('activityRowKey', () => {
  /** The hash alone is NOT unique: one transaction can run several
   *  operations against the same pool, and each is its own row. */
  it('separates two operations of one transaction', () => {
    const hash = 'a'.repeat(64);
    const first = { transaction_hash: hash, operation_index: 0 };
    const second = { transaction_hash: hash, operation_index: 1 };
    expect(activityRowKey(first as PoolActivityItem)).not.toBe(
      activityRowKey(second as PoolActivityItem)
    );
  });
});

describe('PoolActivity table', () => {
  // `asset_type_name` matters: `legHref` keys native routing off it, so a
  // fixture without it renders a plain unlinked code and the link test passes
  // vacuously against nothing.
  const poolItem = {
    legs: [
      { asset_type_name: 'native', asset_code: null, decimals: 7 },
      { asset_type_name: 'classic_credit', asset_code: 'USDC', decimals: 7 },
    ],
  } as PoolItem;

  const makeRow = (over: Partial<PoolActivityItem> = {}): PoolActivityItem => ({
    transaction_hash: 'a'.repeat(64),
    ledger_sequence: 63_904_097,
    operation_index: 0,
    event: 'trade',
    amounts: ['1200000000', '-5000000'],
    source_account: 'G'.repeat(56),
    created_at: '2026-08-11T14:26:36Z',
    ...over,
  });

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
      makeRow({
        operation_index: 0,
        event: 'deposit',
        amounts: ['1200000000', '5000000'],
      }),
      makeRow({ operation_index: 1, event: 'trade' }),
    ]);
    renderWithProviders(<PoolActivity poolId="LPOOL" pool={poolItem} />);

    // Scoped to the table on purpose: the filter control offers the same three
    // words, so an unscoped query matches the button as well as the chip.
    const table = within(screen.getByRole('table'));

    // The bundle that made the per-transaction chip lie: one deposit and one
    // trade in the same transaction, now labelled correctly one row each.
    expect(table.getByText('Deposit')).toBeInTheDocument();
    expect(table.getByText('Trade')).toBeInTheDocument();
    // The amount line renders as parts (digits, icon, linked code), so the
    // joined sentence lives on the line's aria-label — which is also what a
    // screen reader announces for the cell.
    expect(table.getByLabelText('120 XLM + 0.5 USDC')).toBeInTheDocument();
    expect(table.getByLabelText('120 XLM → 0.5 USDC')).toBeInTheDocument();
  });

  it('links a row to its own operation anchor, not just the transaction', () => {
    // The anchor is the operation_index itself: 6 is the page's `#op-6`.
    mockRows([makeRow({ operation_index: 6 })]);
    renderWithProviders(<PoolActivity poolId="LPOOL" pool={poolItem} />);

    const link = screen
      .getAllByRole('link')
      .find((a) => a.getAttribute('href')?.includes('#op-'));
    expect(link?.getAttribute('href')).toBe(
      `/transactions/${'a'.repeat(64)}#op-6`
    );
  });

  it('renders no figure for a row whose legs did not both land', () => {
    mockRows([makeRow({ event: null, amounts: [null, null] })]);
    renderWithProviders(<PoolActivity poolId="LPOOL" pool={poolItem} />);

    // Not a zero and not a dash — "not known" is not "nothing moved".
    expect(screen.queryByText('0 XLM')).not.toBeInTheDocument();
    expect(screen.queryByText('—')).not.toBeInTheDocument();
  });

  /** The way back to everything has to be visible. An earlier cut used a
   *  toggle group, where clearing meant clicking the active button again —
   *  nothing on screen says so. */
  it('offers an explicit way back to all events, selected by default', () => {
    mockRows([makeRow()]);
    renderWithProviders(<PoolActivity poolId="LPOOL" pool={poolItem} />);
    expect(screen.getByText('All events')).toBeInTheDocument();
  });

  /** A 4-token pool's event can name three tokens at most (a soroban event
   *  carries four topics), so one leg of a deposit can be absent. It must
   *  read as absent: no zero for it, no dash, and the row still renders. */
  it('renders a deposit with a missing leg from the legs it has', () => {
    const legs = sorobanPool.legs.map((l) => ({ ...l, decimals: 7 }));
    mockRows([
      makeRow({
        event: 'deposit',
        amounts: ['10000000', '20000000', '30000000', null],
      }),
    ]);
    renderWithProviders(
      <PoolActivity poolId="CPOOL" pool={{ legs } as PoolItem} />
    );
    const table = within(screen.getByRole('table'));
    expect(table.getByText('Deposit')).toBeInTheDocument();
    expect(table.getByLabelText('1 XLM + 2 WETH + 3 USDx')).toBeInTheDocument();
    expect(table.queryByText('ODD')).not.toBeInTheDocument();
    expect(table.queryByText('—')).not.toBeInTheDocument();
  });

  it('marks a multi-pool route hop, and only then', () => {
    mockRows([
      makeRow({ operation_index: 0, pools_crossed: 4 }),
      makeRow({ operation_index: 1, pools_crossed: 1 }),
    ]);
    renderWithProviders(<PoolActivity poolId="LPOOL" pool={poolItem} />);
    expect(screen.getByText('via 4 pools')).toBeInTheDocument();
    expect(screen.queryByText('via 1 pools')).not.toBeInTheDocument();
  });

  it('links a leg to its asset page from the amount cell', () => {
    mockRows([makeRow()]);
    renderWithProviders(<PoolActivity poolId="LPOOL" pool={poolItem} />);
    const link = screen
      .getAllByRole('link')
      .find((a) => a.getAttribute('href') === '/assets/native');
    expect(link).toBeDefined();
  });

  it('says the pool is empty only when no filter is narrowing it', () => {
    mockRows([]);
    renderWithProviders(<PoolActivity poolId="LPOOL" pool={poolItem} />);
    expect(screen.getByText('No activity yet')).toBeInTheDocument();
  });
});
