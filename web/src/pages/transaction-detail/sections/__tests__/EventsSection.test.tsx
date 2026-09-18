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

describe('EventsSection (#378 — the consensus stream is the event list)', () => {
  it('lists the consensus stream on its own, copy excluded', async () => {
    const user = userEvent.setup();
    // Issue #378's transaction in miniature: three consensus events, and a
    // debug channel holding the call trace, a COPY of the contract's transfer,
    // and the resource meter. The page used to advertise all of it as events.
    renderSection(
      [
        event(0, 'fee', { stage: 'before_all_txs' }),
        event(1, 'fee', { stage: 'after_all_txs' }),
        event(2, 'transfer', { op_index: 0 }),
      ],
      [
        event(3, 'fn_call', { event_type: 'diagnostic' }),
        event(6, 'transfer'),
        event(7, 'fn_return', { event_type: 'diagnostic' }),
        event(8, 'core_metrics', { event_type: 'diagnostic' }),
      ]
    );

    expect(screen.getByText('3 events')).toBeInTheDocument();
    await user.click(screen.getByText(/Show 3 events/));

    // The copy is not a fourth event here. It is still on the page, one
    // disclosure down, in the debug channel it actually belongs to.
    expect(rowsOf(screen.getByRole('table')).map((r) => r['#'])).toEqual([
      '0',
      '1',
      '2',
    ]);
  });

  it('keeps the copies in the debug channel — only counters move out', async () => {
    const user = userEvent.setup();
    renderSection(
      [event(2, 'transfer', { op_index: 0 })],
      [
        event(3, 'fn_call', { event_type: 'diagnostic' }),
        event(4, 'transfer'), // the copy — raw record, kept as it arrived
        event(5, 'fn_return', { event_type: 'diagnostic' }),
        event(6, 'core_metrics', { event_type: 'diagnostic' }),
      ]
    );
    await user.click(screen.getByText(/Show 3 diagnostic entries/));

    // The counter is the only omission, and it renders in full on the
    // operation card. Everything else stands exactly as the ledger carries it.
    const table = screen.getAllByRole('table').at(-1) as HTMLElement;
    expect(rowsOf(table).map((r) => r['#'])).toEqual(['3', '4', '5']);
    // …and it states no position: `Where` belongs to the consensus stream.
    expect(rowsOf(table)[0].Where).toBeUndefined();
  });

  it('never lets the debug channel into the event count', () => {
    // The whole of issue #378: two records concatenated into one list and one
    // number, so two events advertised themselves as five.
    renderSection(
      [event(0, 'fee'), event(2, 'transfer')],
      [event(3, 'fn_call'), event(4, 'transfer'), event(5, 'core_metrics')]
    );
    expect(screen.getByText('2 events')).toBeInTheDocument();
    expect(screen.getByText(/Show 2 diagnostic entries/)).toBeInTheDocument();
  });

  it('offers no diagnostics disclosure when counters were all there was', () => {
    // Nothing to disclose once the meter readings render as Resources — an
    // expander onto an empty table would be a dead end.
    renderSection([event(0, 'transfer')], [event(1, 'core_metrics')]);
    expect(screen.queryByText(/diagnostic entr/)).not.toBeInTheDocument();
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

  it('says nothing was emitted when the consensus stream is empty', () => {
    renderSection([], []);
    expect(screen.getByText('No events emitted.')).toBeInTheDocument();
  });
});
