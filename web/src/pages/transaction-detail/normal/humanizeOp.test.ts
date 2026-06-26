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
