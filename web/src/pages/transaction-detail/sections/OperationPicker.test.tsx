import type { OperationItem } from '@rumblefish/api-types';
import { render, screen } from '@testing-library/react';
import { ExplorerThemeProvider } from '@rumblefish/soroban-block-explorer-ui';
import { describe, expect, it, vi } from 'vitest';

import type { OperationEntry } from './operationEntries.js';
import { OperationPicker } from './OperationPicker.js';

const DEST = 'GA5XIGAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAGKTM';

function entry(
  partial: Partial<OperationItem> & { type_name: string },
  order: number
): OperationEntry {
  const light: OperationItem = {
    appearance_id: order,
    type: 1,
    application_order: order,
    ledger_sequence: 1,
    created_at: '2026-01-01T00:00:00Z',
    pool_ids: [],
    ...partial,
  } as OperationItem;
  return { row: light, light, heavy: null };
}

describe('OperationPicker', () => {
  it('shows the humanized sentence as a second line per row (0460 #2)', () => {
    render(
      <ExplorerThemeProvider>
        <OperationPicker
          entries={[
            entry({ type_name: 'PAYMENT', destination_account: DEST }, 1),
          ]}
          txSourceAccount={null}
          selectedIndex={0}
          onSelect={vi.fn()}
        />
      </ExplorerThemeProvider>
    );
    expect(screen.getByText('Payment #1')).toBeTruthy();
    expect(screen.getByText(`Sent XLM to GA5X…GKTM`)).toBeTruthy();
  });

  it('omits the summary when the sentence is the template-less fallback', () => {
    // "X processed" would only repeat the type label above it.
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    render(
      <ExplorerThemeProvider>
        <OperationPicker
          entries={[entry({ type_name: 'SOME_FUTURE_OP' }, 1)]}
          txSourceAccount={null}
          selectedIndex={0}
          onSelect={vi.fn()}
        />
      </ExplorerThemeProvider>
    );
    expect(screen.queryByText(/processed/)).toBeNull();
    warn.mockRestore();
  });
});
