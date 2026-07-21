import { ThemeProvider } from '@mui/material/styles';
import { render, screen, within } from '@testing-library/react';
import { userEvent } from '@testing-library/user-event';
import type { ReactNode } from 'react';
import { describe, expect, it, vi } from 'vitest';

import { createExplorerTheme } from '../theme/theme.js';

import {
  ExplorerTable,
  type ExplorerTableColumn,
  type SortDirection,
} from './ExplorerTable.js';

const THEME = createExplorerTheme('dark');

interface Row {
  id: string;
  hash: string;
  ledger: number;
}

const ROWS: Row[] = [
  { id: 'a', hash: '0xaaa', ledger: 100 },
  { id: 'b', hash: '0xbbb', ledger: 99 },
];

const COLUMNS: ExplorerTableColumn<Row>[] = [
  { id: 'hash', header: 'Hash', cell: (r) => r.hash },
  { id: 'ledger', header: 'Ledger', sortable: true, cell: (r) => r.ledger },
];

function withTheme(ui: ReactNode) {
  return <ThemeProvider theme={THEME}>{ui}</ThemeProvider>;
}

describe('ExplorerTable', () => {
  it('renders semantic table with thead/tbody/th/td', () => {
    render(
      withTheme(
        <ExplorerTable columns={COLUMNS} rows={ROWS} rowKey={(r) => r.id} />
      )
    );
    const table = screen.getByRole('table');
    expect(table).toBeInTheDocument();
    const headers = within(table).getAllByRole('columnheader');
    expect(headers).toHaveLength(2);
    expect(headers[0]).toHaveTextContent('Hash');
    expect(headers[1]).toHaveTextContent('Ledger');
    expect(within(table).getAllByRole('row')).toHaveLength(ROWS.length + 1);
  });

  it('renders each row cell via the column cell renderer', () => {
    render(
      withTheme(
        <ExplorerTable columns={COLUMNS} rows={ROWS} rowKey={(r) => r.id} />
      )
    );
    expect(screen.getByText('0xaaa')).toBeInTheDocument();
    expect(screen.getByText('0xbbb')).toBeInTheDocument();
    expect(screen.getByText('100')).toBeInTheDocument();
    expect(screen.getByText('99')).toBeInTheDocument();
  });

  it('clicking a sortable header fires onSortChange with toggled direction', async () => {
    const user = userEvent.setup();
    const onSortChange = vi.fn<(id: string, dir: SortDirection) => void>();
    render(
      withTheme(
        <ExplorerTable
          columns={COLUMNS}
          rows={ROWS}
          rowKey={(r) => r.id}
          sortBy="ledger"
          sortDir="desc"
          onSortChange={onSortChange}
        />
      )
    );
    const ledgerHeader = screen.getByRole('columnheader', { name: /Ledger/ });
    await user.click(within(ledgerHeader).getByRole('button'));
    expect(onSortChange).toHaveBeenCalledWith('ledger', 'asc');
  });

  it('renders emptyState when rows is empty', () => {
    render(
      withTheme(
        <ExplorerTable
          columns={COLUMNS}
          rows={[]}
          rowKey={(r) => r.id}
          emptyState={<div data-testid="empty">nothing here</div>}
        />
      )
    );
    expect(screen.getByTestId('empty')).toHaveTextContent('nothing here');
  });

  // lore-0420: a ReplacingMergeTree read without deduplication handed the table
  // rows whose `rowKey` collided. React could not match old nodes to new ones and
  // left orphans behind, so re-sorting APPENDED rows without bound. Rendering one
  // row per key removes the collision before React ever sees it.
  it('renders one row per key when rowKey collides across rows', () => {
    const duplicated: Row[] = [
      { id: 'dup', hash: '0xaaa', ledger: 100 },
      { id: 'dup', hash: '0xbbb', ledger: 100 },
      { id: 'dup', hash: '0xccc', ledger: 100 },
    ];

    const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    const { rerender } = render(
      withTheme(
        <ExplorerTable
          columns={COLUMNS}
          rows={duplicated}
          rowKey={(r) => r.id}
        />
      )
    );

    const bodyRows = () =>
      within(screen.getByRole('table')).getAllByRole('row').length - 1;

    // First occurrence survives, the rest are dropped.
    expect(bodyRows()).toBe(1);
    expect(screen.getByText('0xaaa')).toBeInTheDocument();
    expect(screen.queryByText('0xbbb')).not.toBeInTheDocument();

    // Dropping rows hides a backend fault, so it must not be silent.
    expect(warn).toHaveBeenCalledWith(
      expect.stringContaining('duplicate rowKey')
    );

    // Re-render with a different colliding page: rows must be REPLACED, never
    // accumulated (the original bug went 20 -> 30 -> 40).
    rerender(
      withTheme(
        <ExplorerTable
          columns={COLUMNS}
          rows={[
            { id: 'dup', hash: '0xddd', ledger: 99 },
            { id: 'dup', hash: '0xeee', ledger: 99 },
          ]}
          rowKey={(r) => r.id}
        />
      )
    );
    expect(bodyRows()).toBe(1);
    expect(screen.getByText('0xddd')).toBeInTheDocument();
    expect(screen.queryByText('0xaaa')).not.toBeInTheDocument();
    warn.mockRestore();
  });

  it('leaves a table with unique keys untouched', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    render(
      withTheme(
        <ExplorerTable columns={COLUMNS} rows={ROWS} rowKey={(r) => r.id} />
      )
    );
    expect(
      within(screen.getByRole('table')).getAllByRole('row').length - 1
    ).toBe(ROWS.length);
    expect(warn).not.toHaveBeenCalled();
    warn.mockRestore();
  });
});
