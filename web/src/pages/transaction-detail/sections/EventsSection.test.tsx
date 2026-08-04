import type { XdrEventDto } from '@rumblefish/api-types';
import { ExplorerThemeProvider } from '@rumblefish/soroban-block-explorer-ui';
import { render, screen, within } from '@testing-library/react';
import { userEvent } from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';

import { EventsSection } from './EventsSection.js';

function event(
  event_index: number,
  topic0: string | null,
  event_type = 'contract'
): XdrEventDto {
  return {
    event_type,
    contract_id: null,
    topics: topic0 == null ? [] : [{ type: 'sym', value: topic0 }],
    data: { type: 'void' },
    event_index,
    op_index: null,
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

/** Open the collapsed section — everything below lives behind it. */
async function expand(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole('button', { expanded: false }));
}

/** The rendered event rows: their `#` cell and their kind chip. Scoped to the
 *  table body so the column headers and the filter chips cannot answer for
 *  the rows. */
function shownRows(): Array<{ index: string; kind: string }> {
  return screen
    .getAllByRole('row')
    .slice(1) // header
    .map((row) => {
      const cells = within(row).getAllByRole('cell');
      return {
        index: cells[0].textContent ?? '',
        kind: cells[1].textContent ?? '',
      };
    });
}

function shownIndices(): string[] {
  return shownRows().map((r) => r.index);
}

describe('EventsSection (#378 — event taxonomy)', () => {
  it('hides host counters by default and says how many are hidden', async () => {
    const user = userEvent.setup();
    renderSection(
      [event(0, 'transfer')],
      [event(1, 'fn_call'), event(2, 'core_metrics'), event(3, 'core_metrics')]
    );
    await expand(user);

    expect(shownIndices()).toEqual(['0', '1']);
    expect(screen.getByText('2 hidden')).toBeInTheDocument();
    // Never silent: the count the section advertises stays the full one.
    expect(screen.getAllByText(/4 events/).length).toBeGreaterThan(0);
  });

  it('brings the hidden kind back when its chip is clicked', async () => {
    const user = userEvent.setup();
    renderSection([event(0, 'transfer')], [event(1, 'core_metrics')]);
    await expand(user);

    expect(shownIndices()).toEqual(['0']);
    await user.click(screen.getByRole('button', { name: /Host counters 1/ }));
    expect(shownIndices()).toEqual(['0', '1']);
    expect(screen.queryByText(/hidden/)).not.toBeInTheDocument();
  });

  it('keeps one list in transaction event order, never per-kind groups', async () => {
    const user = userEvent.setup();
    // Deliberately handed over out of order — the section must restore the
    // transaction's own numbering, not the order the arrays arrived in.
    renderSection(
      [event(3, 'transfer'), event(0, 'mint')],
      [event(2, 'fn_call'), event(1, 'fn_return')]
    );
    await expand(user);

    expect(shownIndices()).toEqual(['0', '1', '2', '3']);
  });

  it('labels a consensus system event System, not Contract', async () => {
    const user = userEvent.setup();
    renderSection([event(0, 'executable_update', 'system')], []);
    await expand(user);

    expect(shownRows()).toEqual([{ index: '0', kind: 'System' }]);
  });

  it('still renders each effect once when the diagnostic container copies it (0182)', async () => {
    const user = userEvent.setup();
    // The diagnostic container carries a Contract-TYPED copy of the consensus
    // transfer. Classifying on event_type instead of container would show it
    // as a second contract event.
    renderSection(
      [event(0, 'transfer')],
      [event(1, 'transfer', 'contract'), event(2, 'core_metrics')]
    );
    await expand(user);

    expect(shownRows()).toEqual([
      { index: '0', kind: 'Contract' },
      { index: '1', kind: 'Diagnostic' },
    ]);
  });
});
