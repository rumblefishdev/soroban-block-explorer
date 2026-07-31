import type {
  E3ResponseTransactionDetailLight,
  XdrOperationDto,
} from '@rumblefish/api-types';
import { render, screen } from '@testing-library/react';
import { ExplorerThemeProvider } from '@rumblefish/soroban-block-explorer-ui';
import { describe, expect, it } from 'vitest';

import { opFailReason, TransactionSummary } from './TransactionSummary.js';

function op(
  op_type: string,
  application_order: number,
  result_code: string | null
): XdrOperationDto {
  return { op_type, application_order, details: {}, result_code };
}

function tx(
  operations: XdrOperationDto[],
  successful = false
): E3ResponseTransactionDetailLight {
  return {
    hash: 'ab'.repeat(32),
    ledger_sequence: 1,
    created_at: '2026-01-01T00:00:00Z',
    fee_charged: 100,
    successful,
    operation_count: operations.length,
    operations: [],
    heavy: {
      memo_type: null,
      memo: null,
      signatures: [],
      fee_bump_source: null,
      envelope_xdr: null,
      result_xdr: null,
      diagnostic_events: [],
      contract_events: [],
      operations,
      result_code: 'TxFailed',
      operation_tree: null,
    },
    heavy_fields_status: 'ok',
  } as unknown as E3ResponseTransactionDetailLight;
}

describe('opFailReason', () => {
  it('names the first failing op with its code (0352 fixture 7af6d0ed…)', () => {
    const reason = opFailReason(
      tx([
        op('begin_sponsoring_future_reserves', 1, 'Success'),
        op('create_account', 2, 'LowReserve'),
        op('create_account', 3, 'OpNoAccount'),
      ])
    );
    expect(reason).toBe('Create Account #2 — LOW_RESERVE (+1 more failed)');
  });

  it('returns null when codes are absent (old responses, validation failures)', () => {
    expect(opFailReason(tx([op('create_account', 1, null)]))).toBeNull();
    expect(opFailReason(tx([]))).toBeNull();
  });
});

describe('TransactionSummary failed strip', () => {
  it('shows the per-op reason on the strip', () => {
    render(
      <ExplorerThemeProvider>
        <TransactionSummary tx={tx([op('create_account', 1, 'LowReserve')])} />
      </ExplorerThemeProvider>
    );
    expect(screen.getByText(/Create Account #1 — LOW_RESERVE/)).toBeTruthy();
  });
});
