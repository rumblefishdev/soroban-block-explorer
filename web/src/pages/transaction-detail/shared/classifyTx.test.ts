import type { E3ResponseTransactionDetailLight } from '@rumblefish/api-types';
import { describe, expect, it } from 'vitest';

import { classifyTx } from './classifyTx.js';

function tx(typeNames: string[]): E3ResponseTransactionDetailLight {
  return {
    hash: 'x',
    ledger_sequence: 1,
    successful: true,
    operation_count: typeNames.length,
    created_at: '2026-01-01T00:00:00Z',
    operations: typeNames.map((type_name, i) => ({
      appearance_id: i,
      type: 0,
      type_name,
      application_order: i + 1,
      ledger_sequence: 1,
      created_at: '2026-01-01T00:00:00Z',
      pool_ids: [],
    })),
  } as unknown as E3ResponseTransactionDetailLight;
}

describe('classifyTx', () => {
  it('classifies the VELO arbitrage shape as a multi-op swap', () => {
    expect(
      classifyTx(
        tx([
          'CHANGE_TRUST',
          'PATH_PAYMENT_STRICT_RECEIVE',
          'PATH_PAYMENT_STRICT_SEND',
          'CHANGE_TRUST',
        ])
      )
    ).toBe('Swap · 4 ops');
  });

  it('labels invokes, payments, trading and trustline updates', () => {
    expect(classifyTx(tx(['INVOKE_HOST_FUNCTION']))).toBe('Contract call');
    expect(classifyTx(tx(['PAYMENT']))).toBe('Payment');
    expect(classifyTx(tx(['PAYMENT', 'PAYMENT', 'PAYMENT']))).toBe(
      'Payments · 3'
    );
    expect(classifyTx(tx(['MANAGE_SELL_OFFER', 'MANAGE_BUY_OFFER']))).toBe(
      'Trading · 2 ops'
    );
    expect(classifyTx(tx(['CHANGE_TRUST']))).toBe('Trustline update');
  });

  it('stays silent rather than guessing on mixed bags', () => {
    expect(classifyTx(tx(['PAYMENT', 'MANAGE_DATA']))).toBeNull();
    expect(
      classifyTx(tx(['PATH_PAYMENT_STRICT_SEND', 'MANAGE_SELL_OFFER']))
    ).toBeNull();
    expect(classifyTx(tx([]))).toBeNull();
  });
});
