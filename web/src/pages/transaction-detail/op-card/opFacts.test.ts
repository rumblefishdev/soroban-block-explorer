import type { OperationItem, XdrOperationDto } from '@rumblefish/api-types';
import { describe, expect, it } from 'vitest';

import { opFacts } from './opFacts.js';

function light(type_name: string): OperationItem {
  return {
    appearance_id: 1,
    type: 1,
    application_order: 1,
    ledger_sequence: 1,
    created_at: '2026-01-01T00:00:00Z',
    pool_ids: [],
    type_name,
  } as OperationItem;
}

function heavy(details: Record<string, unknown>): XdrOperationDto {
  return { op_type: 'x', application_order: 1, details };
}

describe('opFacts', () => {
  it('adds the D9 Received slot for strict-send (route in RouteStrip, pools as card links)', () => {
    const facts = opFacts(
      light('PATH_PAYMENT_STRICT_SEND'),
      heavy({
        sendAsset: 'native',
        destAsset: 'bubba:GB',
        path: ['TF:GA'],
        poolIds: ['c4f1', '9d02'],
      })
    );
    // "not derivable", not a bare dash: the swap succeeded, so an amount was
    // delivered — we just cannot compute it from the claimed atoms (0377 F7).
    expect(facts).toEqual([{ label: 'Received', value: 'not derivable' }]);
  });

  it('omits the Received slot for strict-receive (exact amount is in the headline)', () => {
    const facts = opFacts(
      light('PATH_PAYMENT_STRICT_RECEIVE'),
      heavy({ sendAsset: 'native', destAsset: 'USDC:GA', path: [] })
    );
    expect(facts).toEqual([]);
  });

  it('returns nothing for non-path-payment types and degraded responses', () => {
    expect(opFacts(light('PAYMENT'), heavy({ amount: 1 }))).toEqual([]);
    expect(opFacts(light('PATH_PAYMENT_STRICT_SEND'), null)).toEqual([]);
  });
});
