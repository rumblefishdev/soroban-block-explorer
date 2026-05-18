import type { TransactionListItem } from '@rumblefish/api-types';
import {
  ExplorerTable,
  IdentifierDisplay,
  IdentifierWithCopy,
  type ExplorerTableColumn,
  type SortDirection,
} from '@rumblefish/soroban-block-explorer-ui';
import { useState } from 'react';

import { Dash, OperationCell, StatusCell } from '../transactions/cells.js';
import { TransactionTime } from '../transactions/TransactionTime.js';

interface LatestTransactionsTableProps {
  rows: readonly TransactionListItem[];
}

const columns: ExplorerTableColumn<TransactionListItem>[] = [
  {
    id: 'hash',
    header: 'Hash',
    cell: (row) => <IdentifierWithCopy value={row.hash} type="transaction" />,
  },
  {
    id: 'source',
    header: 'Source account',
    cell: (row) =>
      row.source_account ? (
        <IdentifierDisplay value={row.source_account} type="account" />
      ) : (
        <Dash />
      ),
  },
  {
    id: 'operation',
    header: 'Operation',
    cell: (row) => <OperationCell types={row.operation_types} />,
  },
  {
    id: 'status',
    header: 'Status',
    cell: (row) => <StatusCell successful={row.successful} />,
  },
  {
    id: 'time',
    header: 'Time',
    sortable: true,
    cell: (row) => <TransactionTime createdAt={row.created_at} />,
  },
];

/** Column count — used to size the loading skeleton consistently. */
export const LATEST_TX_COLUMN_COUNT = columns.length;

/**
 * Home-page Latest Transactions table — hash, source account, operation,
 * status and time. A 5-column subset of the full Transactions list table,
 * per the Figma home design; reuses the shared transaction cells. The Time
 * column is client-side sortable over the fixed set of latest rows.
 */
export function LatestTransactionsTable({
  rows,
}: LatestTransactionsTableProps) {
  const [sort, setSort] = useState<{ by?: string; dir: SortDirection }>({
    dir: 'desc',
  });

  const sortedRows =
    sort.by === 'time'
      ? [...rows].sort((a, b) => {
          const diff =
            new Date(a.created_at).getTime() - new Date(b.created_at).getTime();
          return sort.dir === 'desc' ? -diff : diff;
        })
      : rows;

  return (
    <ExplorerTable
      columns={columns}
      rows={sortedRows}
      rowKey={(row) => row.hash}
      sortBy={sort.by}
      sortDir={sort.dir}
      onSortChange={(id, dir) => setSort({ by: id, dir })}
    />
  );
}
