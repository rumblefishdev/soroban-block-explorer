import type { TransactionListItem } from '@rumblefish/api-types';
import {
  Dash,
  EXPLORER_TABLE_ROW_HEIGHT_TALL,
  ExplorerTable,
  IdentifierWithCopy,
  StatusChip,
  type ExplorerTableColumn,
} from '@rumblefish/soroban-block-explorer-ui';

import { OperationCell } from '../transactions/cells.js';
import { TransactionTime } from '../transactions/TransactionTime.js';

interface LatestTransactionsTableProps {
  rows: readonly TransactionListItem[];
  loading?: boolean;
  skeletonRows?: number;
}

const columns: ExplorerTableColumn<TransactionListItem>[] = [
  {
    id: 'hash',
    header: 'Hash',
    width: 160,
    cell: (row) => <IdentifierWithCopy value={row.hash} type="transaction" />,
  },
  {
    id: 'source',
    header: 'Source account',
    width: 160,
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
    width: 190,
    cell: (row) => <OperationCell types={row.operation_types} />,
  },
  {
    id: 'status',
    header: 'Status',
    width: 120,
    cell: (row) => <StatusChip successful={row.successful} />,
  },
  {
    id: 'time',
    header: 'Time',
    width: 210,
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
  loading,
  skeletonRows,
}: LatestTransactionsTableProps) {
  return (
    <ExplorerTable
      columns={columns}
      rows={rows}
      rowKey={(row) => row.hash}
      rowHeight={EXPLORER_TABLE_ROW_HEIGHT_TALL}
      loading={loading}
      skeletonRows={skeletonRows}
    />
  );
}
