import type { XdrEventDto } from '@rumblefish/api-types';
import { ExplorerThemeProvider } from '@rumblefish/soroban-block-explorer-ui';
import { render, screen, within } from '@testing-library/react';
import { userEvent } from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';

import { EventsSection } from './EventsSection.js';

function event(
  event_index: number,
  topic0: string | null,
  extra: Partial<XdrEventDto> = {}
): XdrEventDto {
  return {
    event_type: 'contract',
    contract_id: null,
    topics: topic0 == null ? [] : [{ type: 'sym', value: topic0 }],
    data: { type: 'void' },
    event_index,
    op_index: null,
    stage: null,
    ...extra,
  } as unknown as XdrEventDto;
}

function renderSection(
  contractEvents: XdrEventDto[],
  diagnosticEvents: XdrEventDto[]
) {
  return render(
    <ExplorerThemeProvider>
      <EventsSection
        contractEvents={contractEvents}
        diagnosticEvents={diagnosticEvents}
      />
    </ExplorerThemeProvider>
  );
}

function rowsOf(table: HTMLElement): Array<Record<string, string>> {
  const head = within(table)
    .getAllByRole('columnheader')
    .map((h) => h.textContent ?? '');
  return within(table)
    .getAllByRole('row')
    .slice(1)
    .map((row) =>
      Object.fromEntries(
        within(row)
          .getAllByRole('cell')
          .map((c, i) => [head[i], (c.textContent ?? '').trim()])
      )
    );
}

describe('EventsSection (#378 — consensus stream vs debug channel)', () => {
  it('counts only the consensus stream, never the debug channel', () => {
    renderSection(
      [event(0, 'fee'), event(2, 'transfer')],
      // Stellar core copies every consensus event into the diagnostic
      // container; merging the two would advertise 5 events for a
      // transaction that emitted 2.
      [event(3, 'fn_call'), event(6, 'transfer'), event(7, 'core_metrics')]
    );

    expect(screen.getByText('2 events')).toBeInTheDocument();
    expect(screen.getByText(/Show 2 events/)).toBeInTheDocument();
    expect(
      screen.getByText(/Show execution diagnostics \(3\)/)
    ).toBeInTheDocument();
  });

  it('keeps the diagnostic copy of an event out of the consensus list', async () => {
    const user = userEvent.setup();
    renderSection(
      [event(2, 'transfer')],
      [event(6, 'transfer'), event(7, 'core_metrics')]
    );
    await user.click(screen.getByText(/Show 2 events|Show 1 event/));

    // One table so far: the consensus one, carrying the single real transfer.
    const rows = rowsOf(screen.getByRole('table'));
    expect(rows).toHaveLength(1);
    expect(rows[0]['#']).toBe('2');
  });

  it('names where a consensus event sits — operation or ledger stage', async () => {
    const user = userEvent.setup();
    renderSection(
      [
        event(0, 'fee', { stage: 'before_all_txs' }),
        event(1, 'fee', { stage: 'after_tx' }),
        event(2, 'transfer', { op_index: 0 }),
      ],
      []
    );
    await user.click(screen.getByText(/Show 3 events/));

    expect(rowsOf(screen.getByRole('table')).map((r) => r.Where)).toEqual([
      'before all txs',
      // The refund fires AFTER the operation below it — the row number is a
      // position in the record, the stage is the time.
      'after tx',
      'op 1',
    ]);
  });

  it('labels a system event System, not Contract', async () => {
    const user = userEvent.setup();
    renderSection(
      [event(0, 'executable_update', { event_type: 'system' })],
      []
    );
    await user.click(screen.getByText(/Show 1 event/));

    expect(rowsOf(screen.getByRole('table'))[0].Type).toBe('System');
  });

  it('hides the host counters inside the debug channel on demand', async () => {
    const user = userEvent.setup();
    renderSection(
      [event(0, 'transfer')],
      [event(1, 'fn_call'), event(2, 'core_metrics'), event(3, 'core_metrics')]
    );
    await user.click(screen.getByText(/Show execution diagnostics \(3\)/));

    const table = screen.getByRole('table');
    expect(rowsOf(table)).toHaveLength(3);

    await user.click(screen.getByRole('button', { name: /Host counters 2/ }));
    expect(rowsOf(screen.getByRole('table')).map((r) => r['#'])).toEqual(['1']);
  });

  it('says nothing was emitted when the consensus stream is empty', () => {
    renderSection([], []);
    expect(screen.getByText('No events emitted.')).toBeInTheDocument();
  });
});
