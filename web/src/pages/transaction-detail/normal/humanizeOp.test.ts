import type { OperationItem, XdrOperationDto } from '@rumblefish/api-types';
import { describe, expect, it } from 'vitest';

import { humanizeOp } from './humanizeOp.js';

/** Minimal `OperationItem` with the fields `humanizeOp` reads; the rest are
 *  filled with inert defaults so the generated type is satisfied. */
function light(
  partial: Partial<OperationItem> & { type_name: string }
): OperationItem {
  return {
    appearance_id: 1,
    type: 1,
    application_order: 1,
    ledger_sequence: 100,
    created_at: '2026-01-01T00:00:00Z',
    pool_ids: [],
    ...partial,
  } as OperationItem;
}

function heavy(details: Record<string, unknown>): XdrOperationDto {
  return { op_type: 'payment', application_order: 1, details };
}

describe('humanizeOp', () => {
  it('shows the sent amount + asset for a native payment', () => {
    const op = light({
      type_name: 'PAYMENT',
      destination_account:
        'GA5XIGAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAGKTM',
    });
    const h = heavy({
      amount: 1_005_000_000,
      asset: 'native',
      destination: 'GA5X',
    });
    expect(humanizeOp(op, h)).toBe('Sent 100.5 XLM to GA5XIG…GKTM');
  });

  it('uses the credit asset code as the unit', () => {
    const op = light({
      type_name: 'PAYMENT',
      destination_account:
        'GA5XIGAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAGKTM',
      asset_code: 'USDC',
    });
    const h = heavy({
      amount: 250_000_000,
      asset: 'USDC:GISSUER',
    });
    expect(humanizeOp(op, h)).toBe('Sent 25 USDC to GA5XIG…GKTM');
  });

  it('reads destAmount/destAsset for a strict-receive path payment', () => {
    const op = light({
      type_name: 'PATH_PAYMENT_STRICT_RECEIVE',
      destination_account:
        'GA5XIGAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAGKTM',
    });
    const h = heavy({ destAmount: 50_000_000, destAsset: 'native' });
    expect(humanizeOp(op, h)).toBe('Sent 5 XLM to GA5XIG…GKTM');
  });

  it('reads sendAmount/sendAsset for a strict-send path payment', () => {
    const op = light({
      type_name: 'PATH_PAYMENT_STRICT_SEND',
      destination_account:
        'GA5XIGAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAGKTM',
    });
    const h = heavy({ sendAmount: 75_000_000, sendAsset: 'BTC:GISSUER' });
    expect(humanizeOp(op, h)).toBe('Sent 7.5 BTC to GA5XIG…GKTM');
  });

  it('renders a zero amount as "0 XLM"', () => {
    const op = light({
      type_name: 'PAYMENT',
      destination_account:
        'GA5XIGAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAGKTM',
    });
    expect(humanizeOp(op, heavy({ amount: 0, asset: 'native' }))).toBe(
      'Sent 0 XLM to GA5XIG…GKTM'
    );
  });

  it('falls back to the asset-only label when heavy lacks the amount field', () => {
    const op = light({
      type_name: 'PAYMENT',
      destination_account:
        'GA5XIGAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAGKTM',
      asset_code: 'USDC',
    });
    // heavy present (e.g. malformed/missing amount) → no amount, asset-only.
    expect(humanizeOp(op, heavy({ asset: 'USDC:GISSUER' }))).toBe(
      'Sent USDC to GA5XIG…GKTM'
    );
  });

  it('falls back to the asset-only label when heavy is unavailable', () => {
    const op = light({
      type_name: 'PAYMENT',
      destination_account:
        'GA5XIGAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAGKTM',
      asset_code: 'EURC',
    });
    expect(humanizeOp(op, null)).toBe('Sent EURC to GA5XIG…GKTM');
  });

  it('shows the starting balance for create-account', () => {
    const op = light({
      type_name: 'CREATE_ACCOUNT',
      destination_account:
        'GA5XIGAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAGKTM',
    });
    const h = heavy({ startingBalance: 100_000_000 });
    expect(humanizeOp(op, h)).toBe('Created account GA5XIG…GKTM with 10 XLM');
  });

  it('falls back to the plain create-account label when heavy lacks startingBalance', () => {
    const op = light({
      type_name: 'CREATE_ACCOUNT',
      destination_account:
        'GA5XIGAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAGKTM',
    });
    expect(humanizeOp(op, heavy({}))).toBe('Created account GA5XIG…GKTM');
  });
});
