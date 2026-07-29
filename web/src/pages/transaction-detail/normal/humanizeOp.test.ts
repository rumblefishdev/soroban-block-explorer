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
    expect(humanizeOp(op, h)).toBe('Sent 100.5 XLM to GA5X…GKTM');
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
    expect(humanizeOp(op, h)).toBe('Sent 25 USDC to GA5X…GKTM');
  });

  it('narrates a strict-receive path payment as a swap with its spend bound', () => {
    const op = light({
      type_name: 'PATH_PAYMENT_STRICT_RECEIVE',
      destination_account:
        'GA5XIGAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAGKTM',
    });
    const h = heavy({
      sendAsset: 'BTC:GISSUER',
      sendMax: 75_000_000,
      destAmount: 50_000_000,
      destAsset: 'native',
    });
    expect(humanizeOp(op, h)).toBe(
      'Swapped BTC → 5 XLM (max 7.5 BTC) for GA5X…GKTM'
    );
  });

  it('narrates a strict-send path payment as a swap, never as a payment', () => {
    const op = light({
      type_name: 'PATH_PAYMENT_STRICT_SEND',
      destination_account:
        'GA5XIGAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAGKTM',
    });
    const h = heavy({
      sendAmount: 75_000_000,
      sendAsset: 'BTC:GISSUER',
      destAsset: 'native',
      destMin: 50_000_000,
    });
    expect(humanizeOp(op, h)).toBe(
      'Swapped 7.5 BTC → XLM (min 5 XLM) for GA5X…GKTM'
    );
  });

  it('drops the "for …" suffix on a self-swap', () => {
    const op = light({
      type_name: 'PATH_PAYMENT_STRICT_SEND',
      source_account: 'GA5XIGAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAGKTM',
      destination_account:
        'GA5XIGAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAGKTM',
    });
    const h = heavy({
      sendAmount: 75_000_000,
      sendAsset: 'BTC:GISSUER',
      destAsset: 'native',
      destMin: 50_000_000,
    });
    expect(humanizeOp(op, h)).toBe('Swapped 7.5 BTC → XLM (min 5 XLM)');
  });

  it('says "to itself" for a self-payment', () => {
    const op = light({
      type_name: 'PAYMENT',
      source_account: 'GA5XIGAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAGKTM',
      destination_account:
        'GA5XIGAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAGKTM',
    });
    const h = heavy({ amount: 1_005_000_000, asset: 'native' });
    expect(humanizeOp(op, h)).toBe('Sent 100.5 XLM to itself');
  });

  it('falls back to the generic label for a path payment without heavy', () => {
    const op = light({
      type_name: 'PATH_PAYMENT_STRICT_SEND',
      destination_account:
        'GA5XIGAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAGKTM',
    });
    expect(humanizeOp(op, null)).toBe('Path Payment Strict Send processed');
  });

  it('renders a zero amount as "0 XLM"', () => {
    const op = light({
      type_name: 'PAYMENT',
      destination_account:
        'GA5XIGAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAGKTM',
    });
    expect(humanizeOp(op, heavy({ amount: 0, asset: 'native' }))).toBe(
      'Sent 0 XLM to GA5X…GKTM'
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
      'Sent USDC to GA5X…GKTM'
    );
  });

  it('falls back to the asset-only label when heavy is unavailable', () => {
    const op = light({
      type_name: 'PAYMENT',
      destination_account:
        'GA5XIGAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAGKTM',
      asset_code: 'EURC',
    });
    expect(humanizeOp(op, null)).toBe('Sent EURC to GA5X…GKTM');
  });

  it('names the called function for invoke-host-function (camelCase key)', () => {
    const op = light({
      type_name: 'INVOKE_HOST_FUNCTION',
      contract_id: 'CDL74RF5BLYR2YBLCCI7F5FB6TPSCLKEJUBSD2RSVWZ4YHF3VMFAIGWA',
    });
    const h = heavy({ functionName: 'plant', functionArgs: [] });
    expect(humanizeOp(op, h)).toBe('Called plant() on CDL7…IGWA');
  });

  it('falls back to the contract-only label when functionName is absent', () => {
    const op = light({
      type_name: 'INVOKE_HOST_FUNCTION',
      contract_id: 'CDL74RF5BLYR2YBLCCI7F5FB6TPSCLKEJUBSD2RSVWZ4YHF3VMFAIGWA',
    });
    expect(humanizeOp(op, heavy({}))).toBe('Invoked contract CDL7…IGWA');
  });

  it('shows the starting balance for create-account', () => {
    const op = light({
      type_name: 'CREATE_ACCOUNT',
      destination_account:
        'GA5XIGAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAGKTM',
    });
    const h = heavy({ startingBalance: 100_000_000 });
    expect(humanizeOp(op, h)).toBe('Created account GA5X…GKTM with 10 XLM');
  });

  it('falls back to the plain create-account label when heavy lacks startingBalance', () => {
    const op = light({
      type_name: 'CREATE_ACCOUNT',
      destination_account:
        'GA5XIGAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAGKTM',
    });
    expect(humanizeOp(op, heavy({}))).toBe('Created account GA5X…GKTM');
  });

  const VELO_ISSUER =
    'GDM4RQUQQUVSKQA7S6EM7XBZP3FCGH4Q7CL6TABQ7B2BEJ5ERARM2M5M';

  it('names the asset and issuer for change-trust (issue #370)', () => {
    const op = light({ type_name: 'CHANGE_TRUST' });
    const h = heavy({
      asset: `VELO:${VELO_ISSUER}`,
      limit: 2 ** 63,
    });
    expect(humanizeOp(op, h)).toBe('Set trustline to VELO (issuer GDM4…2M5M)');
  });

  it('shows a finite change-trust limit', () => {
    const op = light({ type_name: 'CHANGE_TRUST' });
    const h = heavy({ asset: `VELO:${VELO_ISSUER}`, limit: 5_000_000_000 });
    expect(humanizeOp(op, h)).toBe(
      'Set trustline to VELO (issuer GDM4…2M5M) · limit 500 VELO'
    );
  });

  it('reads limit 0 as trustline removal', () => {
    const op = light({ type_name: 'CHANGE_TRUST' });
    const h = heavy({ asset: `VELO:${VELO_ISSUER}`, limit: 0 });
    expect(humanizeOp(op, h)).toBe(
      'Removed trustline to VELO (issuer GDM4…2M5M)'
    );
  });

  it('labels a pool-share trustline without inventing the pair', () => {
    const op = light({ type_name: 'CHANGE_TRUST' });
    const h = heavy({
      asset: { type: 'liquidityPool', params: 'LiquidityPoolConstantProduct' },
      limit: 2 ** 63,
    });
    expect(humanizeOp(op, h)).toBe('Set trustline to liquidity pool shares');
  });

  it('builds the change-trust sentence from light fields when heavy is unavailable', () => {
    const op = light({
      type_name: 'CHANGE_TRUST',
      asset_code: 'VELO',
      asset_issuer: VELO_ISSUER,
    });
    expect(humanizeOp(op, null)).toBe(
      'Set trustline to VELO (issuer GDM4…2M5M)'
    );
  });

  const GTRUSTOR = 'GA5XIGAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAGKTM';

  it('describes a new sell offer with price units', () => {
    const op = light({ type_name: 'MANAGE_SELL_OFFER' });
    const h = heavy({
      selling: 'ETH:GISSUER',
      buying: 'USDC:GISSUER',
      amount: 50_000_000,
      price: { n: 30012, d: 10 },
      offerId: 0,
    });
    expect(humanizeOp(op, h)).toBe(
      'Offered to sell 5 ETH for USDC @ 3,001.2 USDC/ETH'
    );
  });

  it('reads amount 0 on an existing offer as cancellation', () => {
    const op = light({ type_name: 'MANAGE_SELL_OFFER' });
    const h = heavy({
      selling: 'ETH:GISSUER',
      buying: 'USDC:GISSUER',
      amount: 0,
      price: { n: 1, d: 1 },
      offerId: 123,
    });
    expect(humanizeOp(op, h)).toBe('Cancelled offer #123');
  });

  it('describes a buy offer from buyAmount with inverted price units', () => {
    const op = light({ type_name: 'MANAGE_BUY_OFFER' });
    const h = heavy({
      selling: 'USDC:GISSUER',
      buying: 'ETH:GISSUER',
      buyAmount: 50_000_000,
      price: { n: 30012, d: 10 },
      offerId: 0,
    });
    expect(humanizeOp(op, h)).toBe(
      'Offered to buy 5 ETH for USDC @ 3,001.2 USDC/ETH'
    );
  });

  it('labels a passive offer', () => {
    const op = light({ type_name: 'CREATE_PASSIVE_SELL_OFFER' });
    const h = heavy({
      selling: 'ETH:GISSUER',
      buying: 'USDC:GISSUER',
      amount: 50_000_000,
      price: { n: 3, d: 1 },
    });
    expect(humanizeOp(op, h)).toBe(
      'Placed a passive offer: sell 5 ETH for USDC @ 3 USDC/ETH'
    );
  });

  it('names the pool for a liquidity-pool deposit', () => {
    const op = light({ type_name: 'LIQUIDITY_POOL_DEPOSIT' });
    const h = heavy({
      liquidityPoolId:
        'c4f14da0a2c9be653a16bb52345f9e69b2b1e1b0c00c8d94aec6e0006bc07222',
      maxAmountA: 1,
      maxAmountB: 2,
    });
    expect(humanizeOp(op, h)).toBe('Deposited into liquidity pool c4f1…7222');
  });

  it('describes an account merge', () => {
    const op = light({ type_name: 'ACCOUNT_MERGE' });
    const h = heavy({ destination: GTRUSTOR });
    expect(humanizeOp(op, h)).toBe('Merged this account into GA5X…GKTM');
  });

  it('describes escrowing a claimable balance with the claimant count', () => {
    const op = light({ type_name: 'CREATE_CLAIMABLE_BALANCE' });
    const h = heavy({
      asset: 'USDC:GISSUER',
      amount: 50_000_000,
      claimants: 2,
    });
    expect(humanizeOp(op, h)).toBe('Escrowed 5 USDC for 2 claimants');
  });

  it('claims a balance by id only (asset needs meta, spec D8)', () => {
    const op = light({ type_name: 'CLAIM_CLAIMABLE_BALANCE' });
    const h = heavy({
      balanceId:
        '0abc14da0a2c9be653a16bb52345f9e69b2b1e1b0c00c8d94aec6e0006bcef12',
    });
    expect(humanizeOp(op, h)).toBe('Claimed balance 0abc…ef12');
  });

  it('describes a clawback', () => {
    const op = light({ type_name: 'CLAWBACK' });
    const h = heavy({
      asset: 'USDC:GISSUER',
      from: GTRUSTOR,
      amount: 50_000_000,
    });
    expect(humanizeOp(op, h)).toBe('Clawed back 5 USDC from GA5X…GKTM');
  });

  it('reads the AUTHORIZED bit from set-trustline-flags', () => {
    const op = light({ type_name: 'SET_TRUST_LINE_FLAGS' });
    const h = heavy({
      trustor: GTRUSTOR,
      asset: `VELO:${VELO_ISSUER}`,
      setFlags: 1,
      clearFlags: 0,
    });
    expect(humanizeOp(op, h)).toBe('Authorized GA5X…GKTM for VELO');
  });

  it('reads authorize 0 in allow-trust as revocation', () => {
    const op = light({ type_name: 'ALLOW_TRUST' });
    const h = heavy({ trustor: GTRUSTOR, asset: 'VELO', authorize: 0 });
    expect(humanizeOp(op, h)).toBe(
      "Revoked GA5X…GKTM's authorization for VELO"
    );
  });

  it('describes sponsorship begin/end', () => {
    const begin = light({ type_name: 'BEGIN_SPONSORING_FUTURE_RESERVES' });
    expect(humanizeOp(begin, heavy({ sponsoredId: GTRUSTOR }))).toBe(
      'Sponsored reserves for GA5X…GKTM'
    );
    const end = light({ type_name: 'END_SPONSORING_FUTURE_RESERVES' });
    expect(humanizeOp(end, heavy({}))).toBe('Ended reserve sponsorship');
  });

  it('describes revoking a ledger-entry sponsorship', () => {
    const op = light({ type_name: 'REVOKE_SPONSORSHIP' });
    const h = heavy({ kind: 'ledgerEntry', ledgerKeyType: 'Trustline' });
    expect(humanizeOp(op, h)).toBe('Revoked sponsorship of a Trustline entry');
  });

  it('reads signer weight 0 in set-options as signer removal', () => {
    const op = light({ type_name: 'SET_OPTIONS' });
    const h = heavy({ signerKey: GTRUSTOR, signerWeight: 0 });
    expect(humanizeOp(op, h)).toBe('Removed signer GA5X…GKTM');
  });

  it('summarises multi-field set-options generically', () => {
    const op = light({ type_name: 'SET_OPTIONS' });
    const h = heavy({ masterWeight: 1, lowThreshold: 2 });
    expect(humanizeOp(op, h)).toBe('Updated account options');
  });

  it('reads a null manage-data value as deletion', () => {
    const op = light({ type_name: 'MANAGE_DATA' });
    expect(humanizeOp(op, heavy({ name: 'config', value: null }))).toBe(
      'Deleted data entry "config"'
    );
    expect(humanizeOp(op, heavy({ name: 'config', value: 'YWJj' }))).toBe(
      'Set data entry "config"'
    );
  });

  it('covers the remaining short labels', () => {
    expect(
      humanizeOp(light({ type_name: 'BUMP_SEQUENCE' }), heavy({ bumpTo: 42 }))
    ).toBe('Bumped sequence to 42');
    expect(
      humanizeOp(
        light({ type_name: 'EXTEND_FOOTPRINT_TTL' }),
        heavy({ extendTo: 120000 })
      )
    ).toBe('Extended contract state TTL by 120,000 ledgers');
    expect(
      humanizeOp(light({ type_name: 'RESTORE_FOOTPRINT' }), heavy({}))
    ).toBe('Restored archived contract state');
    expect(humanizeOp(light({ type_name: 'INFLATION' }), heavy({}))).toBe(
      'Ran inflation'
    );
  });

  it('falls back to the generic label for every detail-dependent type without heavy', () => {
    for (const type_name of [
      'MANAGE_SELL_OFFER',
      'LIQUIDITY_POOL_DEPOSIT',
      'CREATE_CLAIMABLE_BALANCE',
      'CLAWBACK',
      'REVOKE_SPONSORSHIP',
      'MANAGE_DATA',
    ]) {
      expect(humanizeOp(light({ type_name }), null)).toMatch(/ processed$/);
    }
  });
});
