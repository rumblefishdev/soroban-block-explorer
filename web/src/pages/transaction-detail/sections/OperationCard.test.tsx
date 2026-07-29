import type { OperationItem, XdrOperationDto } from '@rumblefish/api-types';
import { render, screen } from '@testing-library/react';
import { ExplorerThemeProvider } from '@rumblefish/soroban-block-explorer-ui';
import { describe, expect, it } from 'vitest';

import { OperationCard } from './OperationCard.js';

const DEST = 'GA5XIGAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAGKTM';

function light(
  partial: Partial<OperationItem> & { type_name: string }
): OperationItem {
  return {
    appearance_id: 1,
    type: 1,
    application_order: 2,
    ledger_sequence: 1,
    created_at: '2026-01-01T00:00:00Z',
    pool_ids: [],
    ...partial,
  } as OperationItem;
}

function heavyOf(details: Record<string, unknown>): XdrOperationDto {
  return { op_type: 'PAYMENT', application_order: 2, details };
}

function renderCard(props: Partial<Parameters<typeof OperationCard>[0]> = {}) {
  return render(
    <ExplorerThemeProvider>
      <OperationCard
        light={light({ type_name: 'PAYMENT', destination_account: DEST })}
        heavy={heavyOf({ amount: 1_005_000_000, asset: 'native' })}
        applied
        defaultDetailsOpen={false}
        fallbackOrder={1}
        txSourceAccount={null}
        {...props}
      />
    </ExplorerThemeProvider>
  );
}

describe('OperationCard', () => {
  it('renders the headline sentence, order and type label', () => {
    renderCard();
    expect(screen.getByText('Sent 100.5 XLM to GA5X…GKTM')).toBeTruthy();
    expect(screen.getByText('2 · Payment')).toBeTruthy();
  });

  it('labels the card "not applied" and keeps the disclosure on a failed transaction', () => {
    renderCard({ applied: false });
    expect(screen.getByText('not applied')).toBeTruthy();
    expect(
      screen.getByRole('button', { name: /Operation details/ })
    ).toBeTruthy();
  });

  it('opens the raw details by default in advanced mode', () => {
    renderCard({ defaultDetailsOpen: true });
    expect(
      screen
        .getByRole('button', { name: /Operation details/ })
        .getAttribute('aria-expanded')
    ).toBe('true');
  });
});
