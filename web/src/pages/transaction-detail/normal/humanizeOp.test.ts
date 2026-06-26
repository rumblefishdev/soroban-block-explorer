import type { OperationItem, XdrOperationDto } from '@rumblefish/api-types';
import { describe, expect, it } from 'vitest';

import { humanizeOp } from './humanizeOp.js';

function light(partial: Partial<OperationItem>): OperationItem {
  return {
    appearance_id: 1,
    created_at: '2026-01-01T00:00:00Z',
    ledger_sequence: 100,
    pool_ids: [],
    type: 1,
    type_name: 'PAYMENT',
    ...partial,
  };
}

function heavy(details: unknown): XdrOperationDto {
  return { application_order: 1, op_type: 'payment', details };
}

const DEST = 'GA5XQF7C4DTYUBLImaginedDestinationAccountAddress1234567890XY';

describe('humanizeOp — payment amount from heavy.details', () => {
  it('renders native amount as XLM, divided by 1e7', () => {
    const line = humanizeOp(
      light({ destination_account: DEST }),
      heavy({ amount: 125_000_000, asset: 'native', destination: DEST })
    );
    expect(line).toContain('Sent 12.5 XLM to');
  });

  it('renders a non-native asset with its code', () => {
    const line = humanizeOp(
      light({ destination_account: DEST }),
      heavy({ amount: 50_000_000, asset: `USDC:${DEST}`, destination: DEST })
    );
    expect(line).toContain('Sent 5 USDC to');
  });

  it('renders a strict-receive path payment from destAmount/destAsset', () => {
    const line = humanizeOp(
      light({
        type_name: 'PATH_PAYMENT_STRICT_RECEIVE',
        destination_account: DEST,
      }),
      {
        application_order: 1,
        op_type: 'path_payment_strict_receive',
        details: {
          destAmount: 73_500_000,
          destAsset: `AQUA:${DEST}`,
          destination: DEST,
        },
      }
    );
    expect(line).toContain('Sent 7.35 AQUA to');
  });

  it('renders a strict-send path payment from sendAmount/sendAsset', () => {
    const line = humanizeOp(
      light({
        type_name: 'PATH_PAYMENT_STRICT_SEND',
        destination_account: DEST,
      }),
      {
        application_order: 1,
        op_type: 'path_payment_strict_send',
        details: {
          sendAmount: 100_000_000,
          sendAsset: 'native',
          destination: DEST,
        },
      }
    );
    expect(line).toContain('Sent 10 XLM to');
  });

  it('falls back to asset/destination only when heavy amount is absent', () => {
    const line = humanizeOp(light({ destination_account: DEST }), null);
    expect(line).toContain('Sent XLM to');
    expect(line).not.toContain('NaN');
  });

  it('prefers an explicit heavy summary string when present', () => {
    const line = humanizeOp(
      light({ destination_account: DEST }),
      heavy({ summary: 'Custom summary', amount: 1, asset: 'native' })
    );
    expect(line).toBe('Custom summary');
  });
});

describe('humanizeOp — amount on other operation types', () => {
  it('CREATE_ACCOUNT shows the starting balance in XLM', () => {
    const line = humanizeOp(
      light({ type_name: 'CREATE_ACCOUNT' }),
      heavy({ destination: DEST, startingBalance: 250_000_000 })
    );
    expect(line).toContain('Created account');
    expect(line).toContain('with 25 XLM');
  });

  it('CREATE_ACCOUNT without heavy still names the account, no amount', () => {
    const line = humanizeOp(
      light({ type_name: 'CREATE_ACCOUNT', destination_account: DEST }),
      null
    );
    expect(line).toContain('Created account');
    expect(line).not.toContain('with');
  });

  it('CLAWBACK shows amount + asset + source', () => {
    const line = humanizeOp(
      light({ type_name: 'CLAWBACK' }),
      heavy({ amount: 50_000_000, asset: `USDC:${DEST}`, from: DEST })
    );
    expect(line).toContain('Clawed back 5 USDC from');
  });

  it('CREATE_CLAIMABLE_BALANCE shows amount + asset', () => {
    const line = humanizeOp(
      light({ type_name: 'CREATE_CLAIMABLE_BALANCE' }),
      heavy({ amount: 1_000_000_000, asset: `AQUA:${DEST}` })
    );
    expect(line).toContain('Created claimable balance of 100 AQUA');
  });

  it('MANAGE_SELL_OFFER shows amount, counter-asset and price', () => {
    const line = humanizeOp(
      light({ type_name: 'MANAGE_SELL_OFFER' }),
      heavy({
        amount: 1_000_000_000,
        selling: 'native',
        buying: `USDC:${DEST}`,
        price: { n: 1, d: 2 },
      })
    );
    expect(line).toBe('Sell offer: 100 XLM for USDC @ 0.5');
  });

  it('MANAGE_SELL_OFFER with amount 0 reads as a cancellation', () => {
    const line = humanizeOp(
      light({ type_name: 'MANAGE_SELL_OFFER' }),
      heavy({
        amount: 0,
        selling: `POOL:${DEST}`,
        buying: 'native',
        offerId: 1844659947,
        price: { n: 9997979, d: 10000001 },
      })
    );
    expect(line).toBe('Cancelled sell offer (POOL/XLM)');
  });

  it('MANAGE_BUY_OFFER reads buyAmount and the buying asset', () => {
    const line = humanizeOp(
      light({ type_name: 'MANAGE_BUY_OFFER' }),
      heavy({
        buyAmount: 30_000_000,
        selling: 'native',
        buying: `USDC:${DEST}`,
        price: { n: 2, d: 1 },
      })
    );
    expect(line).toBe('Buy offer: 3 USDC for XLM @ 2');
  });

  it('LIQUIDITY_POOL_DEPOSIT shows both max amounts (no asset codes)', () => {
    const line = humanizeOp(
      light({ type_name: 'LIQUIDITY_POOL_DEPOSIT' }),
      heavy({ maxAmountA: 100_000_000, maxAmountB: 50_000_000 })
    );
    expect(line).toBe('Deposited up to 10 / 5');
  });

  it('LIQUIDITY_POOL_WITHDRAW shows the pool-share amount', () => {
    const line = humanizeOp(
      light({ type_name: 'LIQUIDITY_POOL_WITHDRAW' }),
      heavy({ amount: 70_000_000, minAmountA: 1, minAmountB: 1 })
    );
    expect(line).toContain('Withdrew 7 pool shares');
  });

  it('falls back to "<Type> processed" when heavy is absent', () => {
    expect(humanizeOp(light({ type_name: 'MANAGE_SELL_OFFER' }), null)).toBe(
      'Manage Sell Offer processed'
    );
  });
});
