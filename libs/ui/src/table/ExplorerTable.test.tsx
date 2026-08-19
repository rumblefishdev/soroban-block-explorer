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
});

describe('hideBelow', () => {
  /** MUI's `useMediaQuery` consults `window.matchMedia`; jsdom has none, so
   *  the default is "no match" — desktop. Stubbing it to match every
   *  `(max-width…)` query simulates the narrow viewport. */
  function stubNarrowViewport() {
    vi.stubGlobal(
      'matchMedia',
      (query: string) =>
        ({
          matches: query.includes('max-width'),
          media: query,
          addEventListener: () => undefined,
          removeEventListener: () => undefined,
          addListener: () => undefined,
          removeListener: () => undefined,
          onchange: null,
          dispatchEvent: () => false,
        } as MediaQueryList)
    );
  }

  const hideable: ExplorerTableColumn<Row>[] = [
    { id: 'hash', header: 'Hash', cell: (r) => r.hash },
    {
      id: 'ledger',
      header: 'Ledger',
      hideBelow: 'sm',
      cell: (r) => r.ledger,
    },
  ];

  it('keeps every column on a desktop viewport', () => {
    render(
      withTheme(
        <ExplorerTable columns={hideable} rows={ROWS} rowKey={(r) => r.id} />
      )
    );
    expect(screen.getByText('Ledger')).toBeInTheDocument();
  });

  it('drops a hideBelow column under the breakpoint, minWidth included', () => {
    stubNarrowViewport();
    try {
      render(
        withTheme(
          <ExplorerTable columns={hideable} rows={ROWS} rowKey={(r) => r.id} />
        )
      );
      expect(screen.queryByText('Ledger')).not.toBeInTheDocument();
      // The header cell count is what drives layout — one column, not a
      // hidden gap.
      expect(screen.getAllByRole('columnheader')).toHaveLength(1);
    } finally {
      vi.unstubAllGlobals();
    }
  });
});
