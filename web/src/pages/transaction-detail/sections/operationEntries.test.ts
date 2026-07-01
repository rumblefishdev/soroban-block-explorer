import type {
  E3ResponseTransactionDetailLight,
  OperationItem,
  XdrOperationDto,
} from '@rumblefish/api-types';
import { describe, expect, it } from 'vitest';

import { buildOperationEntries } from './operationEntries.js';

function light(partial: Partial<OperationItem>): OperationItem {
  return {
    appearance_id: 1,
    created_at: '2026-01-01T00:00:00Z',
    ledger_sequence: 100,
    pool_ids: [],
    type: 0,
    type_name: 'PAYMENT',
    ...partial,
  };
}

function heavy(application_order: number, op_type: string): XdrOperationDto {
  return { application_order, op_type, details: { amount: application_order } };
}

function tx(
  operations: OperationItem[],
  heavyOps: XdrOperationDto[] | null
): E3ResponseTransactionDetailLight {
  return {
    created_at: '2026-01-01T00:00:00Z',
    ledger_sequence: 100,
    operations,
    operation_count: heavyOps?.length ?? operations.length,
    heavy: heavyOps ? { operations: heavyOps } : null,
  } as unknown as E3ResponseTransactionDetailLight;
}

describe('buildOperationEntries', () => {
  it('unfolds: 4 heavy ops over 1 folded light row -> 4 entries', () => {
    const entries = buildOperationEntries(
      tx(
        [
          light({
            appearance_id: 7,
            application_order: 1,
            type_name: 'MANAGE_BUY_OFFER',
          }),
        ],
        [1, 2, 3, 4].map((n) => heavy(n, 'manage_buy_offer'))
      )
    );
    expect(entries).toHaveLength(4);
    // every entry resolves to the shared light identity but carries its own heavy op
    expect(entries.map((e) => e.heavy?.application_order)).toEqual([
      1, 2, 3, 4,
    ]);
    expect(entries.every((e) => e.light?.appearance_id === 7)).toBe(true);
    // picker keys (row.appearance_id) are unique
    const keys = entries.map((e) => e.row.appearance_id);
    expect(new Set(keys).size).toBe(4);
  });

  it('matches 1:1 unfolded ops by application_order', () => {
    const entries = buildOperationEntries(
      tx(
        [
          light({
            appearance_id: 1,
            application_order: 1,
            type_name: 'PAYMENT',
          }),
          light({
            appearance_id: 2,
            application_order: 2,
            type_name: 'CREATE_ACCOUNT',
          }),
        ],
        [heavy(1, 'payment'), heavy(2, 'create_account')]
      )
    );
    expect(entries.map((e) => e.light?.type_name)).toEqual([
      'PAYMENT',
      'CREATE_ACCOUNT',
    ]);
  });

  it('synthesizes a row when no light identity matches', () => {
    const entries = buildOperationEntries(tx([], [heavy(1, 'bump_sequence')]));
    expect(entries).toHaveLength(1);
    expect(entries[0]?.light).toBeUndefined();
    expect(entries[0]?.row.type_name).toBe('BUMP_SEQUENCE');
  });

  it('falls back to folded light rows when heavy is absent', () => {
    const ops = [light({ appearance_id: 5, application_order: 1 })];
    const entries = buildOperationEntries(tx(ops, null));
    expect(entries).toHaveLength(1);
    expect(entries[0]?.heavy).toBeNull();
    expect(entries[0]?.light?.appearance_id).toBe(5);
  });
});
