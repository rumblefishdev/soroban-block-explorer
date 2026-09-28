import type { XdrEventDto } from '@rumblefish/api-types';
import { ExplorerThemeProvider } from '@rumblefish/soroban-block-explorer-ui';
import { render, screen, within } from '@testing-library/react';
import { userEvent } from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';

import { EventsSection } from '../EventsSection.js';

/** An rpc id as `getEvents` returns it, for ledger 64,450,000 (ADR 0059). */
function rpcId(transaction: number, operation: number, event: number): string {
  const toid =
    (BigInt(64_450_000) << 32n) |
    (BigInt(transaction) << 12n) |
    BigInt(operation);
  return `${toid.toString().padStart(19, '0')}-${String(event).padStart(
    10,
    '0'
  )}`;
}

/** The transaction of these fixtures: application order 136. */
const CHARGE = rpcId(0, 0, 135);
const OP_EVENT = rpcId(136, 0, 0);
const REFUND = rpcId(1_048_575, 0, 0);

function event(
  id: string | null,
  topic0: string | null,
  extra: Partial<XdrEventDto> = {}
): XdrEventDto {
  return {
    event_type: 'contract',
    contract_id: null,
    topics: topic0 == null ? [] : [{ type: 'sym', value: topic0 }],
    data: { type: 'void' },
    id,
    event_index: null,
    operation_index: null,
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
        event(CHARGE, 'fee', { stage: 'before_all_txs' }),
        event(OP_EVENT, 'transfer', { operation_index: 0, event_index: 0 }),
        event(REFUND, 'fee', { stage: 'after_all_txs' }),
      ],
      [
        event(null, 'fn_call', { event_type: 'diagnostic' }),
        event(null, 'transfer', { event_type: 'diagnostic' }),
        event(null, 'fn_return', { event_type: 'diagnostic' }),
        event(null, 'core_metrics', { event_type: 'diagnostic' }),
      ]
    );

    expect(screen.getByText('3 events')).toBeInTheDocument();
    await user.click(screen.getByText(/Show 3 events/));

    // The copy is not a fourth event here. It is still on the page, one
    // disclosure down, in the debug channel it actually belongs to.
    expect(rowsOf(screen.getByRole('table')).map((r) => r.ID)).toEqual([
      CHARGE,
      OP_EVENT,
      REFUND,
    ]);
  });

  it('keeps the copies in the debug channel — only counters move out', async () => {
    const user = userEvent.setup();
    renderSection(
      [event(OP_EVENT, 'transfer', { operation_index: 0, event_index: 0 })],
      [
        event(null, 'fn_call', { event_type: 'diagnostic' }),
        event(null, 'transfer'), // the copy — raw record, kept as it arrived
        event(null, 'fn_return', { event_type: 'diagnostic' }),
        event(null, 'core_metrics', { event_type: 'diagnostic' }),
      ]
    );
    await user.click(screen.getByText(/Show 3 diagnostic entries/));

    // The counter is the only omission, and it renders in full on the
    // operation card. Everything else stands exactly as the ledger carries it.
    const table = screen.getAllByRole('table').at(-1) as HTMLElement;
    // A diagnostic event has no rpc id — the protocol gives it none.
    expect(rowsOf(table).map((r) => r.ID)).toEqual(['—', '—', '—']);
    // …and it states no position: `Where` belongs to the consensus stream.
    expect(rowsOf(table)[0].Where).toBeUndefined();
  });

  it('never lets the debug channel into the event count', () => {
    // The whole of issue #378: two records concatenated into one list and one
    // number, so two events advertised themselves as five.
    renderSection(
      [event(CHARGE, 'fee'), event(OP_EVENT, 'transfer')],
      [
        event(null, 'fn_call'),
        event(null, 'transfer'),
        event(null, 'core_metrics'),
      ]
    );
    expect(screen.getByText('2 events')).toBeInTheDocument();
    expect(screen.getByText(/Show 2 diagnostic entries/)).toBeInTheDocument();
  });

  it('offers no diagnostics disclosure when counters were all there was', () => {
    // Nothing to disclose once the meter readings render as Resources — an
    // expander onto an empty table would be a dead end.
    renderSection([event(OP_EVENT, 'transfer')], [event(null, 'core_metrics')]);
    expect(screen.queryByText(/diagnostic entr/)).not.toBeInTheDocument();
  });

  it('names where a consensus event sits — operation or ledger stage', async () => {
    const user = userEvent.setup();
    renderSection(
      [
        event(CHARGE, 'fee', { stage: 'before_all_txs' }),
        event(rpcId(136, 4095, 0), 'fee', { stage: 'after_tx' }),
        event(OP_EVENT, 'transfer', { operation_index: 0, event_index: 0 }),
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
      [event(OP_EVENT, 'executable_update', { event_type: 'system' })],
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
