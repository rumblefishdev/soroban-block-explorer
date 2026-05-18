import type { LedgerListItem } from '@rumblefish/api-types';
import {
  Chip,
  ExplorerTable,
  IdentifierDisplay,
  IdentifierWithCopy,
  type ExplorerTableColumn,
  type SortDirection,
} from '@rumblefish/soroban-block-explorer-ui';
import { Typography } from '@mui/material';
import { useState } from 'react';

import { TransactionTime } from '../transactions/TransactionTime.js';

interface LedgersTableProps {
  rows: readonly LedgerListItem[];
  /**
   * Enable client-side sorting on the Sequence column. Used by the home
   * page Latest Ledgers table, which renders a fixed set of rows.
   */
  sortable?: boolean;
}

const columns: ExplorerTableColumn<LedgerListItem>[] = [
  {
    id: 'sequence',
    header: 'Sequence',
    cell: (row) => (
      <IdentifierDisplay
        value={String(row.sequence)}
        type="ledger"
        truncate={false}
      />
    ),
  },
  {
    id: 'hash',
    header: 'Hash',
    // Middle-truncated, not linked — a ledger has no hash route, and the
    // `ledger` entity type only truncates sequence numbers.
    cell: (row) => (
      <IdentifierWithCopy
        value={row.hash}
        type="ledger"
        linked={false}
        truncation={{ prefix: 6, suffix: 4 }}
      />
    ),
  },
  {
    id: 'closed_at',
    header: 'Closed at',
    cell: (row) => <TransactionTime createdAt={row.closed_at} />,
  },
  {
    id: 'protocol',
    header: 'Protocol',
    cell: (row) => (
      <Chip size="sm" color="neutral" label={String(row.protocol_version)} />
    ),
  },
  {
    id: 'tx_count',
    header: 'TX Count',
    align: 'right',
    cell: (row) => (
      <Typography component="span" variant="bodySmRegular">
        {row.transaction_count.toLocaleString('en-US')}
      </Typography>
    ),
  },
];

/** Columns with the Sequence header marked sortable. */
const sortableColumns: ExplorerTableColumn<LedgerListItem>[] = columns.map(
  (col) => (col.id === 'sequence' ? { ...col, sortable: true } : col)
);

/** Column count — used to size the loading skeleton consistently. */
export const LEDGER_COLUMN_COUNT = columns.length;

/**
 * The Ledgers list table — sequence, hash, closed-at, protocol and
 * transaction-count columns, per the Figma design. Pass `sortable` to
 * enable Sequence sorting (home page Latest Ledgers).
 */
export function LedgersTable({ rows, sortable = false }: LedgersTableProps) {
  const [sortDir, setSortDir] = useState<SortDirection>('desc');

  if (!sortable) {
    return (
      <ExplorerTable
        columns={columns}
        rows={rows}
        rowKey={(row) => String(row.sequence)}
      />
    );
  }

  const sortedRows = [...rows].sort((a, b) =>
    sortDir === 'desc' ? b.sequence - a.sequence : a.sequence - b.sequence
  );

  return (
    <ExplorerTable
      columns={sortableColumns}
      rows={sortedRows}
      rowKey={(row) => String(row.sequence)}
      sortBy="sequence"
      sortDir={sortDir}
      onSortChange={(_id, dir) => setSortDir(dir)}
    />
  );
}
