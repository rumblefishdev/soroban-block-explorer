import type { E3ResponseTransactionDetailLight } from '@rumblefish/api-types';
import { render, screen } from '@testing-library/react';
import { ExplorerThemeProvider } from '@rumblefish/soroban-block-explorer-ui';
import { describe, expect, it, vi } from 'vitest';

import { OperationsSection } from './OperationsSection.js';

function tx(
  heavy: unknown,
  operationCount: number
): E3ResponseTransactionDetailLight {
  return {
    hash: 'ab'.repeat(32),
    ledger_sequence: 1,
    created_at: '2026-01-01T00:00:00Z',
    fee_charged: 100,
    successful: true,
    operation_count: operationCount,
    operations: [
      {
        appearance_id: 1,
        created_at: '2026-01-01T00:00:00Z',
        ledger_sequence: 1,
        pool_ids: [],
        type: 0,
        type_name: 'PAYMENT',
      },
    ],
    heavy,
    heavy_fields_status: heavy == null ? 'unavailable' : 'ok',
  } as unknown as E3ResponseTransactionDetailLight;
}

function renderSection(t: E3ResponseTransactionDetailLight) {
  return render(
    <ExplorerThemeProvider>
      <OperationsSection tx={t} selectedIndex={0} onSelect={vi.fn()} />
    </ExplorerThemeProvider>
  );
}

/**
 * With no archive data the picker silently falls back to the folded light rows
 * and the card's trace, authorized calls, events and route strip all resolve
 * empty — four absences with no hint, so an invoke reads as "made no sub-calls
 * and emitted no events". One line has to say otherwise (0377 F7).
 */
describe('OperationsSection archive warning', () => {
  it('warns when there is no archive data at all', () => {
    renderSection(tx(null, 1));

    expect(screen.getByText(/Execution detail unavailable/)).toBeTruthy();
  });

  // The same symptom from the other cause: the archive answered, but this
  // transaction's envelope was missing, so operations came back empty. Testing
  // `heavy == null` alone would miss it.
  it('warns when the archive answered but carried no operations', () => {
    renderSection(tx({ operations: [] }, 1));

    expect(screen.getByText(/Execution detail unavailable/)).toBeTruthy();
  });

  it('names the shortfall when the header counts more than the picker can list', () => {
    renderSection(tx(null, 4));

    expect(screen.getByText(/only 1 of 4 operations/)).toBeTruthy();
  });

  it('stays quiet when the archive data is present', () => {
    renderSection(
      tx({ operations: [{ op_type: 'payment', application_order: 1 }] }, 1)
    );

    expect(screen.queryByText(/Execution detail unavailable/)).toBeNull();
  });
});
