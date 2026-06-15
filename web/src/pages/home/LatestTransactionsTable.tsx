import type { TransactionListItem } from '@rumblefish/api-types';
import {
  Dash,
  ExplorerTable,
  IdentifierWithCopy,
  StatusChip,
  type ExplorerTableColumn,
} from '@rumblefish/soroban-block-explorer-ui';

import { OperationCell } from '../transactions/cells.js';
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
        <IdentifierWithCopy value={row.source_account} type="account" />
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
    cell: (row) => <StatusChip successful={row.successful} />,
  },
  {
    id: 'time',
    header: 'Time',
    cell: (row) => <TransactionTime createdAt={row.created_at} />,
  },
];

/** Column count — used to size the loading skeleton consistently. */
export const LATEST_TX_COLUMN_COUNT = columns.length;

/**
 * Home-page Latest Transactions table — hash, source account, operation,
 * status and time. A 5-column subset of the full Transactions list table,
 * per the Figma home design; reuses the shared transaction cells.
 */
export function LatestTransactionsTable({
  rows,
}: LatestTransactionsTableProps) {
  return (
    <ExplorerTable columns={columns} rows={rows} rowKey={(row) => row.hash} />
  );
}
